#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const MODEL: &str = include_str!("support/attribute_assignment/model.rb");
const CONTRACT: &str = include_str!("support/attribute_assignment/contract.rb");

fn app(model: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            include_str!("support/attribute_assignment/schema.rb"),
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\nend\n",
        )
        .write("app/models/attribute_record.rb", model)
}

#[test]
fn hash_assignment_uses_writers_in_order_without_saving() {
    app(MODEL).run_ruby(CONTRACT).assert_passes();
}

#[test]
fn a_source_assignment_method_keeps_its_implementation() {
    let model = MODEL.replace("class AttributeRecord < ApplicationRecord", "class AttributeRecord < ApplicationRecord\n  def assign_attributes(attributes)\n    self.state = \"owned\"\n    :owned\n  end");
    app(&model).run_ruby("record = AttributeRecord.new\nraise \"override return\" unless record.assign_attributes(state: \"caller\") == :owned\nraise \"override body\" unless record.state == \"owned\"\n").assert_passes();
}

#[test]
fn a_strict_target_reports_attribute_assignment() {
    let (_, errors) = app(MODEL).emit(roundhouse::project::BuildTarget::Rust);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("assign_attributes")),
        "{errors:?}"
    );
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn hash_assignment_runs_with_native_rbs() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{CONTRACT}"
    );
    app(MODEL).run_spinel_with_rbs(&script).assert_passes();
}

fn has_json_app() -> emit_and_run::Overlay {
    app(include_str!("support/attribute_assignment/has_json_model.rb")).write(
        "db/schema.rb",
        "ActiveRecord::Schema.define do\n  create_table :attribute_records do |t|\n    t.json :settings\n  end\nend\n",
    )
}

#[test]
fn hash_assignment_casts_and_merges_declared_json_keys() {
    has_json_app()
        .run_ruby(include_str!(
            "support/attribute_assignment/has_json_contract.rb"
        ))
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn declared_json_assignment_runs_with_native_rbs() {
    let script = format!(
        "Db.configure(\":memory:\")\nSchema.statements.each {{ |sql| Db.exec(sql) }}\nActiveRecord.adapter = SqliteAdapter\n{}",
        include_str!("support/attribute_assignment/has_json_contract.rb")
    );
    has_json_app().run_spinel_with_rbs(&script).assert_passes();
}
