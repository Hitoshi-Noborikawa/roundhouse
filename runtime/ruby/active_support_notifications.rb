# frozen_string_literal: true
# `ActiveSupport::Notifications`. Subscribe for the block, unsubscribe
# after. A one-argument callback receives an Event; any other arity
# receives the five arguments Rails passes. `monotonic:` is accepted
# and ignored: this runtime has one clock.
#
# `"sql.active_record"` is not instrumented by the connection. That
# name reads `Db.capture_sql`, which records every statement, and the
# callback runs after the block. The payload carries `:sql` and
# `:name` (`"SQL"`).
module ActiveSupport
  module Notifications
    SUBSCRIBERS = []

    def self.subscribe(pattern, callback = nil, monotonic: false, &block)
      _ = monotonic
      listener = callback.nil? ? block : callback
      subscriber = [pattern, listener]
      SUBSCRIBERS << subscriber
      subscriber
    end

    def self.unsubscribe(subscriber)
      SUBSCRIBERS.delete(subscriber)
      nil
    end

    def self.instrument(name, payload = nil, &block)
      payload = {} if payload.nil?
      value = block.nil? ? nil : yield
      i = 0
      now = Time.now
      while i < SUBSCRIBERS.length
        entry = SUBSCRIBERS[i]
        pattern = entry[0]
        callback = entry[1]
        if pattern === name
          if callback.arity == 1
            callback.call(Event.new(name, now, now, nil, payload))
          else
            callback.call(name, now, now, nil, payload)
          end
        end
        i += 1
      end
      value
    end

    def self.subscribed(callback, name, monotonic: false)
      _ = monotonic
      if name == "sql.active_record"
        result = nil
        statements = Db.capture_sql { result = yield }
        i = 0
        while i < statements.length
          callback.call(name, nil, nil, nil, { sql: statements[i], name: "SQL" })
          i += 1
        end
        return result
      end
      subscriber = subscribe(name, callback)
      begin
        yield
      ensure
        unsubscribe(subscriber)
      end
    end

    class Event
      attr_reader :name, :time, :end, :transaction_id, :payload

      def initialize(name, start, ending, transaction_id, payload)
        @name = name
        @time = start
        @end = ending
        @transaction_id = transaction_id
        @payload = payload
      end
    end
  end
end
