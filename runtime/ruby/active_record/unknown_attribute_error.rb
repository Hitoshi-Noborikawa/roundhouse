# frozen_string_literal: true
# ActiveModel::UnknownAttributeError, with Rails 8.1.4's message.
#
# Ruby-family home (off the strict-target tables, unlike errors.rb), as
# `stale_object_error.rb`: its message reads `record.class.name`, which the
# strict targets' transpiled errors file could not compile (C#, Kotlin,
# Swift and Crystal refused it there), and only the Ruby family's mass
# assignment raises it.
module ActiveModel
  class UnknownAttributeError < NoMethodError
    attr_reader :record, :attribute

    def initialize(record, attribute)
      @record = record
      @attribute = attribute
      super("unknown attribute '#{attribute}' for #{record.class.name}.")
    end
  end
end
