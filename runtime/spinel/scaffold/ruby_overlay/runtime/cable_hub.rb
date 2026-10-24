# frozen_string_literal: true

# Cross-worker broadcasts for the CRuby overlay.
#
# `Cable::Registry` (cable.rb) knows only the sockets of the process it
# lives in. With one worker that is every socket; with several forked
# workers a message posted in one reaches only the people whose socket
# landed in the same one, and the rest never see it. Campfire's own
# deployment solves this with Redis behind Action Cable; this solves it
# with one more process on the same box, the shape falcon.rb already uses
# for the WAL checkpointer.
#
#   Hub.serve(path)       the hub process: a Unix socket that relays every
#                         line a worker writes to every other worker
#   Hub.connect(path)     a worker's transport for Broadcasts.set_transport:
#                         delivers to its own sockets at once, writes the
#                         message to the hub for the others, and hands what
#                         the hub relays to its own Registry
#
# The wire is newline-delimited JSON, `{"s": stream, "m": message}`. A
# message is a rendered <turbo-stream> String or an
# `ActionCable.server.broadcast` Hash; both survive JSON unchanged, as far
# as `Registry.deliver` can tell (it re-encodes either with JSON.generate,
# and reads an internal disconnect's `type` under a String or Symbol key).
#
# Each publisher's messages reach every worker in the order it sent them:
# one socket per worker, one relay thread per worker in the hub. Messages
# from different workers have no order between them, as with Redis.
require "socket"
require "json"

module Cable
  class Hub
    # How long a worker waits for the hub to start listening: the container
    # spawns the hub and the workers at the same time.
    CONNECT_TIMEOUT = 10

    def self.serve(path)
      File.unlink(path) if File.exist?(path)
      server = UNIXServer.new(path)
      clients = []
      lock = Mutex.new
      loop do
        client = server.accept
        lock.synchronize { clients << client }
        Thread.new(client) do |source|
          source.each_line do |line|
            targets = lock.synchronize { clients - [source] }
            targets.each do |target|
              target.write(line)
            rescue SystemCallError, IOError
              lock.synchronize { clients.delete(target) }
            end
          end
        rescue SystemCallError, IOError
          nil
        ensure
          lock.synchronize { clients.delete(source) }
          source.close unless source.closed?
        end
      end
    end

    def self.connect(path, local = Registry)
      deadline = Process.clock_gettime(Process::CLOCK_MONOTONIC) + CONNECT_TIMEOUT
      begin
        new(UNIXSocket.new(path), local)
      rescue Errno::ENOENT, Errno::ECONNREFUSED
        raise if Process.clock_gettime(Process::CLOCK_MONOTONIC) > deadline
        sleep 0.05
        retry
      end
    end

    def initialize(socket, local)
      @socket = socket
      @local = local
      @lock = Mutex.new
      # A thread, not a fiber on the worker's event loop: it spends its life
      # blocked in a read, and hands each message to the Registry, which
      # posts it onto the cable reactor's own thread.
      @reader = Thread.new { relay_in }
    end

    # The Broadcasts transport. The local delivery comes first and does not
    # wait on the hub.
    def broadcast(stream, message)
      @local.broadcast(stream, message)
      line = JSON.generate({ "s" => stream, "m" => message }) << "\n"
      @lock.synchronize { @socket.write(line) }
      nil
    rescue SystemCallError, IOError => e
      warn "[cable hub] publish: #{e.class}: #{e.message}"
      nil
    end

    private
      def relay_in
        @socket.each_line do |line|
          envelope = JSON.parse(line)
          @local.broadcast(envelope["s"], envelope["m"])
        end
      rescue SystemCallError, IOError, JSON::ParserError => e
        warn "[cable hub] relay: #{e.class}: #{e.message}"
      end
  end
end
