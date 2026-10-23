//! Not a published gem: the shapes rolify's `rolify`/`resourcify` take, small enough to read in one place.

use super::emit_and_run;

const GEM: &str = "vendor/bundle/ruby/3.4.0/gems/taggy-0.1.0/lib";

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("Gemfile.lock", "    debug (1.11.1)\n", "    taggy (0.1.0)\n    debug (1.11.1)\n")
        .edit("Gemfile.lock", "DEPENDENCIES\n", "DEPENDENCIES\n  taggy\n")
        .write(".bundle/config", "---\nBUNDLE_PATH: \"vendor/bundle\"\n")
        .write("roundhouse.yml", "extensions:\n  - config/roundhouse\n")
        .write("config/roundhouse/gems.yml", "source_gems:\n  - {gem: \"taggy\"}\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n", "class Article < ApplicationRecord\n  taggy\n")
        .write(
            &format!("{GEM}/taggy.rb"),
            r#"require "taggy/config"
require "taggy/tagged"
require "taggy/railtie" if defined?(Rails)

module Taggy
  extend Config

  attr_accessor :tag_cname, :tag_table
  @@kinds = []

  def taggy(options = {})
    include Tagged
    options.reverse_merge!({ cname: "Label" })
    self.tag_cname = options[:cname]
    self.tag_table = tag_cname.tableize.gsub(/\//, "_")
    extra = options.reject { |k, _v| ![:prefix].include?(k.to_sym) }
    @@kinds << self.name if Taggy.registry
  end

  def self.kinds
    @@kinds
  end
end
"#,
        )
        .write(
            &format!("{GEM}/taggy/config.rb"),
            "module Taggy\n  module Config\n    @@registry = true\n\n    def registry\n      @@registry\n    end\n\n    \
             def registry=(value)\n      @@registry = value\n    end\n\n    def disable_registry\n      self.registry = false\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEM}/taggy/tagged.rb"),
            "module Taggy\n  module Tagged\n    def tag_label\n      \"#{self.class.tag_cname}:#{self.class.tag_table}\"\n    end\n  end\nend\n",
        )
        .write(
            &format!("{GEM}/taggy/railtie.rb"),
            "module Taggy\n  class Railtie < Rails::Railtie\n    initializer \"taggy.active_record\" do\n      \
             ActiveSupport.on_load(:active_record) do\n        ActiveRecord::Base.send :extend, Taggy\n      end\n    end\n  end\nend\n",
        )
}

#[test]
fn a_hook_extended_macro_runs_as_rails_runs_it() {
    app()
        .run_ruby(
            r#"
a = Article.create!(title: "Hello world", body: "body number one is long enough")
raise "tag_label: #{a.tag_label.inspect}" unless a.tag_label == "Label:labels"
raise "kinds: #{Taggy.kinds.inspect}" unless Taggy.kinds == ["Article"]
puts "ok"
"#,
        )
        .assert_passes();
}
