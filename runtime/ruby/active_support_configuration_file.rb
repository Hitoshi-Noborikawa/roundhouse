# frozen_string_literal: true
require "erb"
require "yaml"

# `ActiveSupport::ConfigurationFile.parse(path)` — read a YAML file,
# run it through ERB when it contains a tag, and allow YAML aliases.
# An empty document is `{}`. Symbolizing keys is the caller's.
module ActiveSupport
  class ConfigurationFile
    def self.parse(path)
      text = File.read(path)
      text = ERB.new(text).result if text.include?("<%")
      loaded = YAML.safe_load(text, aliases: true)
      return {} if loaded.nil?
      loaded
    end
  end
end
