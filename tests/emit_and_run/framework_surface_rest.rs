//! `ActiveRecord::Type::String` and `Type::Integer` cast on the ruby target.

use super::emit_and_run;

#[test]
fn active_record_string_and_integer_types_cast() {
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
