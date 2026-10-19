//! Type casters, thread-local state, notifications, the null cache,
//! configuration files, and the ActiveJob constants, on the ruby target.

use super::emit_and_run;

fn shell() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        )
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
}

#[test]
fn active_record_string_and_integer_types_cast() {
    shell()
        .write(
            "app/models/caster.rb",
            r#"class Caster
  def self.values
    [
      ActiveRecord::Type::String.new.cast(nil),
      ActiveRecord::Type::String.new.cast(12),
      ActiveRecord::Type::Integer.new.cast(""),
      ActiveRecord::Type::Integer.new.cast(true),
      ActiveRecord::Type::Integer.new.deserialize("12"),
    ]
  end
end
"#,
        )
        .run_ruby(
            r#"
got = Caster.values
raise got.inspect unless got == [nil, "12", nil, 1, 12]
"#,
        )
        .assert_passes();
}

#[test]
fn support_constants_run() {
    shell()
        .write(
            "app/models/surface.rb",
            r#"class Surface
  def self.state
    ActiveSupport::IsolatedExecutionState[:box] = "kept"
    ActiveSupport::IsolatedExecutionState[:box]
  end

  def self.heard
    heard = []
    ActiveSupport::Notifications.subscribe("demo") { |*args| heard << args[0] }
    ActiveSupport::Notifications.instrument("demo") { "ok" }
    heard
  end

  def self.cached
    store = ActiveSupport::Cache::NullStore.new
    store.write("k", "v")
    [store.read("k"), store.fetch("k") { "fresh" }]
  end

  def self.config(path)
    ActiveSupport::ConfigurationFile.parse(path)
  end
end
"#,
        )
        .run_ruby(
            r#"
raise Surface.state.inspect unless Surface.state == "kept"
raise Surface.heard.inspect unless Surface.heard == ["demo"]
raise Surface.cached.inspect unless Surface.cached == [nil, "fresh"]
path = "config/probe.yml"
File.write(path, "size: <%= 1 + 1 %>\n")
raise Surface.config(path).inspect unless Surface.config(path)["size"] == 2
"#,
        )
        .assert_passes();
}
