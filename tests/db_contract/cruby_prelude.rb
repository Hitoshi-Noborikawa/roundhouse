# CRuby lanes: the runtime's own ActiveRecord error classes and the
# connection pool db_cruby.rb leases from, loaded from the checkout
# (ROUNDHOUSE_ROOT) rather than copied.
root = ENV.fetch("ROUNDHOUSE_ROOT")
require File.join(root, "runtime/ruby/active_record/errors")
require File.join(root, "runtime/ruby/active_record/connection_pool")

module ActiveRecord
  # runtime/ruby/active_record/connection.rb defines it with the rest of
  # the transaction surface this suite does not load.
  class Rollback < StandardError
  end
end
