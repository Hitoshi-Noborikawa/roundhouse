# frozen_string_literal: true
# CRuby-only ActiveRecord::Base extensions: `find_by!` + record equality.
#
# Lives on the CRuby overlay, not shared runtime/ruby/active_record/base.rb:
# the shared Base transpiles method-by-method into every target's model
# modules, and the strict targets don't carry a generic `find_by/1` for it
# to call (elixir CI: `undefined function find_by/1` in every model).
# Class-method inheritance makes this reach every model on CRuby. When
# lobsters comes up on another target, that target grows its own variant.
module ActiveRecord
  class Base
    def self.find_by!(conditions)
      result = find_by(conditions)
      raise RecordNotFound.new("Couldn't find #{name}", name, primary_key) if result.nil?
      result
    end

    # Rails record equality: same class + same persisted id (AR core's
    # `==`). Without it, `@showing_user != @user` compares OBJECT
    # IDENTITY — two hydrations of the same row are never equal, so
    # users/show rendered its own-profile "Send a Message" subnav for
    # everyone. `eql?`/`hash` follow AR core so Hash/Set membership
    # agrees with `==`.
    #
    # AR core guards on `!id.nil?`, but an unsaved record's `id` is 0 here
    # (roundhouse#1478), so that guard let any two new records of a class
    # be equal and hash alike: `[a, b].uniq` kept one. The guard is "this
    # record was saved" instead. That's `persisted?`, or `destroyed?`: a
    # destroyed record keeps its id, and Rails keeps it equal to a load of
    # its row.
    def ==(other)
      super ||
        (other.instance_of?(self.class) && (persisted? || destroyed?) && other.id == id)
    end
    alias eql? ==

    def hash
      persisted? || destroyed? ? [self.class, id].hash : super
    end
  end
end
