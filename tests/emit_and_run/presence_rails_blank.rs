use super::emit_and_run;

/// `presence:` and `absence:` with Rails' `blank?`: a String of Unicode
/// whitespace (`[[:space:]]`, so U+3000 and U+00A0 as well as ASCII) is
/// blank, and so is `false` (a boolean column's `false` fails presence),
/// while `0` is not. The check only treated nil and `""` (and an empty
/// Array) as blank. Answers are Rails 8.1.4's for the same model and records.
fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"items\", force: :cascade do |t|\n    t.string \"name\"\n    t.boolean \"live\"\n    t.integer \"count\"\n    t.string \"gone\"\n  end\nend\n",
        )
        .write(
            "app/models/item.rb",
            "class Item < ApplicationRecord\n  validates :name, presence: true\n  validates :live, presence: true\n  validates :count, presence: true\n  validates :gone, absence: true\nend\n",
        )
}

#[test]
fn presence_and_absence_use_rails_blank() {
    app()
        .run_ruby(
            r#"rows = [
  [nil, nil, nil, nil],
  ["", false, 0, ""],
  ["  ", true, 5, "  "],
  ["　", false, nil, "　"],
  [" ", true, 1, " "],
  ["x", true, 2, "x"],
]
got = rows.map do |name, live, count, gone|
  r = Item.new
  r.name = name
  r.live = live
  r.count = count
  r.gone = gone
  r.valid?
  r.errors.to_a
end
want = [
  ["Name can't be blank", "Live can't be blank", "Count can't be blank"],
  ["Name can't be blank", "Live can't be blank"],
  ["Name can't be blank"],
  ["Name can't be blank", "Live can't be blank", "Count can't be blank"],
  ["Name can't be blank"],
  ["Gone must be blank"],
]
raise "errors: #{got.inspect}" unless got == want
puts "rails blank passed"
"#,
        )
        .assert_passes();
}

#[test]
fn strict_targets_emit_the_blank_test_without_errors() {
    use roundhouse::project::BuildTarget;
    for target in [BuildTarget::Rust, BuildTarget::Typescript, BuildTarget::Go, BuildTarget::Crystal] {
        let (_emitted, errors) = app().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
    }
}
