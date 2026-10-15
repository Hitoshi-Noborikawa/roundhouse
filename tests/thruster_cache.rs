//! ThrusterCache: Thruster's response cache, in process, for the CRuby
//! overlay. Each rule the module's header claims, held to it.

use std::process::Command;

const SCRIPT: &str = r##"
require "rack"
require File.expand_path("runtime/spinel/scaffold/ruby_overlay/runtime/thruster_cache", Dir.pwd)

$clock = 1000.0
Process.singleton_class.prepend(Module.new do
  def clock_gettime(id, *rest)
    id == Process::CLOCK_MONOTONIC ? $clock : super
  end
end)

calls = Hash.new(0)
responses = {
  "/public" => [200, { "content-type" => "text/css", "cache-control" => "public, max-age=60",
                       "set-cookie" => "s=1", "etag" => "\"v1\"" }, ["body-public"]],
  "/private" => [200, { "cache-control" => "max-age=0, private, must-revalidate" }, ["body-private"]],
  "/nocache" => [200, { "cache-control" => "public, no-cache, max-age=60" }, ["n"]],
  "/zero" => [200, { "cache-control" => "public, max-age=0" }, ["z"]],
  "/star" => [200, { "cache-control" => "public, max-age=60", "vary" => "*" }, ["s"]],
  "/notfound" => [404, { "cache-control" => "public, max-age=60" }, ["nf"]],
  "/vary" => [200, { "cache-control" => "public, max-age=60", "vary" => "Accept" }, nil],
  "/big" => [200, { "cache-control" => "public, max-age=60" }, ["x" * ((1 << 20) + 1)]],
  "/hijack" => [-1, {}, []],
}
app = lambda do |env|
  path = env["PATH_INFO"]
  calls[path] += 1
  status, headers, body = responses.fetch(path)
  body ||= ["accept=#{env["HTTP_ACCEPT"]}"]
  [status, headers.dup, body.dup]
end
mw = ThrusterCache::Middleware.new(app)
get = ->(path, extra = {}) { mw.call(Rack::MockRequest.env_for(path, extra)) }
check = ->(cond, what) { raise "FAILED: #{what}" unless cond }

# stored once, answered from memory after; Set-Cookie gone from both
s, h, b = get.("/public")
check.(s == 200 && h["x-cache"] == "miss" && b.join == "body-public", "first public GET is a miss")
check.(!h.key?("set-cookie"), "a stored response's miss loses Set-Cookie")
check.(h["vary"] == "Accept-Encoding", "a stored response without Vary gains Vary: Accept-Encoding")
s, h, b = get.("/public")
check.(calls["/public"] == 1 && h["x-cache"] == "hit" && b.join == "body-public", "second public GET is a hit")
check.(!h.key?("set-cookie"), "a hit carries no Set-Cookie")

# 304 for a matching If-None-Match; HEAD has its own key and no body
s, h, b = get.("/public", "HTTP_IF_NONE_MATCH" => "\"v0\", \"v1\"")
check.(s == 304 && b.empty? && !h.key?("content-type"), "a hit answers 304 to its ETag")
get.("/public", method: "HEAD")
s, h, b = get.("/public", method: "HEAD")
check.(s == 200 && h["x-cache"] == "hit" && b.empty?, "HEAD hit has no body")

# expiry
$clock += 61
s, h, = get.("/public")
check.(h["x-cache"] == "miss" && calls["/public"] == 3, "an expired entry is fetched again")

# never stored
%w[/private /nocache /zero /star /notfound /big].each do |path|
  2.times { get.(path) }
  check.(calls[path] == 2, "#{path} is not stored")
end
s, h, b = get.("/big")
check.(b.join.bytesize == (1 << 20) + 1, "an oversized body is still served whole")

# bypass: methods, Range, upgrade, long URIs, disabled
s, h, = mw.call(Rack::MockRequest.env_for("/public", method: "POST"))
check.(h["x-cache"] == "bypass", "POST bypasses")
before = calls["/public"]
get.("/public", "HTTP_RANGE" => "bytes=0-1")
get.("/public", "HTTP_UPGRADE" => "websocket")
get.("/public?" + "q" * 2048)
check.(calls["/public"] == before + 3, "Range, upgrade and a long URI bypass")
ENV["ROUNDHOUSE_RESPONSE_CACHE"] = "0"
get.("/public")
check.(calls["/public"] == before + 4, "ROUNDHOUSE_RESPONSE_CACHE=0 turns it off")
ENV.delete("ROUNDHOUSE_RESPONSE_CACHE")

# Vary: keyed by the request's value for each named header
a1 = get.("/vary", "HTTP_ACCEPT" => "text/a")[2].join
b1 = get.("/vary", "HTTP_ACCEPT" => "text/b")[2].join
a2 = get.("/vary", "HTTP_ACCEPT" => "text/a")[2].join
b2 = get.("/vary", "HTTP_ACCEPT" => "text/b")[2].join
check.(a1 == "accept=text/a" && b1 == "accept=text/b", "variants are not crossed")
check.(a2 == a1 && b2 == b1 && calls["/vary"] == 2, "each variant is stored once")

# a hijacked response passes through untouched
s, h, = get.("/hijack")
check.(s == -1 && h.empty?, "a hijack tuple is left alone")

# capacity: least recently used evicted first
ThrusterCache.instance_variable_set(:@items, {})
ThrusterCache.instance_variable_set(:@size, 0)
ThrusterCache.send(:remove_const, :CAPACITY)
ThrusterCache.const_set(:CAPACITY, 2000) # three ~630-byte entries fit, four do not
3.times { |i| responses["/lru#{i}"] = [200, { "cache-control" => "public, max-age=60" }, ["y" * 300]] }
responses["/lru3"] = [200, { "cache-control" => "public, max-age=60" }, ["y" * 300]]
%w[/lru0 /lru1 /lru2 /lru0 /lru3].each { |p| get.(p) }
get.("/lru0"); get.("/lru1")
check.(calls["/lru0"] == 1 && calls["/lru1"] == 2, "the least recently used entry is evicted")

puts "ALL OK"
"##;

#[test]
fn thruster_cache_follows_thrusters_rules() {
    let output = Command::new("ruby")
        .args(["-e", SCRIPT])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Ruby interpreter available");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "thruster cache failed:\n{stdout}\n{stderr}");
    assert!(stdout.contains("ALL OK"), "thruster cache cases did not execute:\n{stdout}");
}
