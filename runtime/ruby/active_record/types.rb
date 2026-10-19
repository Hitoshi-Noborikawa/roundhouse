# frozen_string_literal: true
# ActiveRecord::Type::String and Type::Integer. Boolean stays on the
# cast lowerer (`ActiveSupport.cast_boolean`); this file does not
# define it. A cast answers nil for a blank value and the typed value
# otherwise. `deserialize` casts the same way.
module ActiveRecord
  module Type
    class String
      def cast(value)
        return nil if value.nil?
        value.to_s
      end

      def deserialize(value)
        cast(value)
      end
    end

    class Integer
      def cast(value)
        return nil if value.nil? || value == ""
        return 1 if value == true
        return 0 if value == false
        return value if value.is_a?(::Integer)
        value.to_i
      end

      def deserialize(value)
        cast(value)
      end
    end
  end
end
