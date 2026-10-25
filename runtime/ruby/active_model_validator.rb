# frozen_string_literal: true
# `ActiveModel::Validator`, the base of a `validates_with` class
# (`app/validators/*.rb`): built with the declaration's options, then
# `validate(record)` adds the record's errors. The ruby family's
# `validates_with` expansion (`ingest::validates_with`) calls
# `<Validator>.new(options).validate(self)` from the model's validation.
module ActiveModel
  class Validator
    def initialize(options = {})
      @options = options
    end

    def options
      @options
    end

    # A subclass defines it; the base adds nothing.
    def validate(record)
      nil
    end
  end
end
