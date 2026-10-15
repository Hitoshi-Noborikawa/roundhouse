# frozen_string_literal: true

# Thruster's response cache, in process, for the CRuby overlay.
#
# Campfire's Dockerfile runs `thrust` in front of Puma, and Thruster keeps
# every response that says `Cache-Control: public` with a positive
# max-age in memory and answers repeats itself: the avatar route
# (`expires_in 30.minutes, public: true`) and every `/assets/*` file
# (`config.public_file_server.headers`) after their first request. The
# rails lane on the bench page gets that; the emitted tree served each
# repeat through the app again.
#
# IN PROCESS, NOT A PROXY. A proxy is a second process on the same cores
# and a second hop per request: Thruster's own cache hits top out near
# 35k req/s on the bench box (the rails lane's stylesheet), half of what
# the in-process Ruby port of campfire serves the same file at. The port
# (tobi/campfire-once-ruby-ractor, lib/campfire/front.rb) carries
# Thruster's rules into the app process; so does this, after it.
#
# Thruster's rules (internal/cache_handler.go, memory_cache.go):
#   * GET/HEAD only; not a WebSocket upgrade, not a Range request, and a
#     path+query of at most 2,048 bytes;
#   * stored when the status is 2xx/3xx other than 304, Cache-Control
#     says `public` with a positive `s-max-age` or `max-age` and no
#     `no-cache`, and Vary is not `*`; kept for that many seconds;
#   * a stored response loses its Set-Cookie (so does the miss that
#     stored it), and gains `Vary: Accept-Encoding` if it had no Vary;
#   * a Vary'd response is keyed by the request's values for those
#     headers, found through the entry at the plain key;
#   * a hit answers 304 to an If-None-Match naming its ETag;
#   * `X-Cache: hit | miss | bypass` on everything;
#   * items up to 1 MiB, 64 MiB in all (CACHE_SIZE), least recently used
#     evicted first.
#
# One store per worker PROCESS, as the port keeps one per worker Ractor,
# with capacity split across WEB_CONCURRENCY when the environment says how
# many there are. A worker's threads share it under a mutex.
#
# ROUNDHOUSE_RESPONSE_CACHE=0 turns it off, for A/B measurement.
#
# Not `ResponseCache`: campfire's own app/models/response_cache.rb is a
# top-level class of that name, and the two would collide at boot.
module ThrusterCache
  MAX_CACHEABLE_URI = 2048
  MAX_ITEM = 1 << 20
  ENTRY_OVERHEAD = 256
  CAPACITY = Integer(ENV.fetch("CACHE_SIZE", (64 << 20).to_s)) /
    [ENV.fetch("WEB_CONCURRENCY", "1").to_i, 1].max
  HIT = "hit"
  MISS = "miss"
  BYPASS = "bypass"

  Entry = Struct.new(:status, :headers, :body, :variant, :expires_at, :size)

  @items = {} # insertion-ordered: least recently used first
  @size = 0
  @mutex = Mutex.new

  def self.enabled?
    ENV["ROUNDHOUSE_RESPONSE_CACHE"] != "0"
  end

  # Rack middleware; outermost, so it answers ahead of the static file
  # server as Thruster does ahead of Puma.
  class Middleware
    def initialize(app)
      @app = app
    end

    def call(env)
      ThrusterCache.call(@app, env)
    end
  end

  def self.call(app, env)
    method = env["REQUEST_METHOD"]
    unless (method == "GET" || method == "HEAD") && cacheable_request?(env)
      return mark(app.call(env), BYPASS)
    end
    now = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    key = base_key(env)
    entry = get(key, now)
    if entry && entry.variant && !variant_matches?(entry.variant, env)
      key = variant_key(key, entry.variant, env)
      entry = get(key, now)
    end
    return hit(entry, env) if entry

    status, headers, body = app.call(env)
    return [status, headers, body] if status.to_i < 0 # hijacked
    lifetime = cache_lifetime(status, headers)
    return mark([status, headers, body], MISS) unless lifetime

    stored = {}
    headers.each { |name, value| stored[name.downcase] = value }
    stored.delete("set-cookie")
    stored["vary"] = "Accept-Encoding" unless stored["vary"]
    text = read_body(body)
    response_headers = stored.merge("x-cache" => MISS)
    return [status, response_headers, body] if text.nil?
    record(key, env, status, stored, text, now + lifetime)
    [status, response_headers, [text]]
  end

  def self.cacheable_request?(env)
    return false unless enabled?
    return false if env["HTTP_UPGRADE"].to_s.downcase == "websocket"
    return false if env["HTTP_CONNECTION"].to_s == "Upgrade"
    return false unless env["HTTP_RANGE"].to_s.empty?
    env["PATH_INFO"].to_s.bytesize + env["QUERY_STRING"].to_s.bytesize < MAX_CACHEABLE_URI
  end

  def self.base_key(env)
    query = env["QUERY_STRING"].to_s
    path = query.empty? ? env["PATH_INFO"].to_s : "#{env["PATH_INFO"]}?#{query}"
    "#{env["REQUEST_METHOD"]}\n#{path}\n#{env["HTTP_HOST"]}"
  end

  def self.request_header(env, name)
    env["HTTP_#{name.upcase.tr("-", "_")}"].to_s
  end

  def self.variant_key(key, variant, env)
    variant.reduce(+key) { |k, (name, _)| k << "\n" << name << "=" << request_header(env, name) }
  end

  def self.variant_matches?(variant, env)
    variant.all? { |name, value| request_header(env, name) == value }
  end

  # Lifetime in seconds, or nil when Thruster would not store it.
  def self.cache_lifetime(status, headers)
    status = status.to_i
    return nil if status < 200 || status > 399 || status == 304
    vary = header(headers, "vary")
    return nil if vary&.include?("*")
    cc = header(headers, "cache-control") or return nil
    return nil unless cc.match?(/\bpublic\b/) && !cc.match?(/\bno-cache\b/)
    m = cc.match(/\bs-max-age=(\d+)\b/) || cc.match(/\bmax-age=(\d+)\b/) or return nil
    seconds = m[1].to_i
    seconds > 0 ? seconds : nil
  end

  def self.header(headers, name)
    headers[name] || headers[name.split("-").map(&:capitalize).join("-")]
  end

  # The whole body, or nil when it is known too large to store: an Array
  # (still servable as it is) or a file it never opened. An iterated body
  # is read whole, and `set` refuses it if it turns out too large.
  def self.read_body(body)
    if body.is_a?(Array)
      text = body.length == 1 ? body[0].to_s : body.join
      body.close if body.respond_to?(:close)
      return text.bytesize <= MAX_ITEM ? text : nil
    end
    if body.respond_to?(:to_path) && File.file?(body.to_path) && File.size(body.to_path) > MAX_ITEM
      return nil
    end
    parts = []
    body.each { |part| parts << part.to_s }
    body.close if body.respond_to?(:close)
    parts.join
  end

  def self.record(key, env, status, headers, text, expires_at)
    variant = vary_names(headers)&.map { |name| [name, request_header(env, name)].freeze }&.freeze
    size = text.bytesize + key.bytesize + ENTRY_OVERHEAD
    headers.each { |name, value| size += name.bytesize + value.to_s.bytesize }
    set(key, Entry.new(status, headers.freeze, text.frozen? ? text : text.dup.freeze, variant, expires_at, size))
  end

  def self.vary_names(headers)
    vary = headers["vary"]
    return nil if vary.nil? || vary.empty?
    vary.split(",").map! { |n| n.strip.downcase }.sort!
  end

  def self.hit(entry, env)
    headers = entry.headers.merge("x-cache" => HIT)
    status = entry.status
    body = entry.body
    etag = headers["etag"]
    inm = env["HTTP_IF_NONE_MATCH"]
    if etag && inm && inm.split(",").any? { |candidate| candidate.strip == etag }
      status = 304
      body = nil
      headers.delete("content-type")
      headers.delete("content-length")
    end
    body = nil if env["REQUEST_METHOD"] == "HEAD"
    [status, headers, body.nil? || body.empty? ? [] : [body]]
  end

  def self.mark(response, value)
    status, headers, body = response
    return response if status.to_i < 0 # hijacked
    begin
      headers["x-cache"] = value
    rescue FrozenError
      headers = headers.merge("x-cache" => value)
    end
    [status, headers, body]
  end

  def self.get(key, now)
    @mutex.synchronize do
      entry = @items[key] or return nil
      if entry.expires_at < now
        @items.delete(key)
        @size -= entry.size
        return nil
      end
      @items.delete(key)
      @items[key] = entry # most recently used last
      entry
    end
  end

  def self.set(key, entry)
    return if entry.size > MAX_ITEM || entry.size > CAPACITY
    @mutex.synchronize do
      if (old = @items.delete(key))
        @size -= old.size
      end
      while @size + entry.size > CAPACITY && (oldest = @items.each_key { |k| break k })
        @size -= @items.delete(oldest).size
      end
      @items[key] = entry
      @size += entry.size
    end
  end
end
