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

/// A nullable `date` or `datetime` column keeps its text in
/// `@<attr>_raw`, so presence and absence read that slot: a set date is
/// present and a set time is not absent. Answers are Rails 8.1.4's.
fn temporal_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"tasks\", force: :cascade do |t|\n    t.date \"due_on\"\n    t.datetime \"closed_at\"\n  end\nend\n",
        )
        .write(
            "app/models/task.rb",
            "class Task < ApplicationRecord\n  validates :due_on, presence: true\n  validates :closed_at, absence: true\nend\n",
        )
}

#[test]
fn nullable_temporal_columns_read_their_stored_text() {
    temporal_app()
        .run_ruby(
            r#"rows = [[nil, nil], ["2026-10-21", "2026-10-21 10:00:00"], ["2026-10-21", nil]]
got = rows.map do |due_on, closed_at|
  t = Task.new
  t.due_on = due_on
  t.closed_at = closed_at
  t.valid?
  t.errors.to_a
end
want = [["Due on can't be blank"], ["Closed at must be blank"], []]
raise "errors: #{got.inspect}" unless got == want
puts "temporal presence/absence passed"
"#,
        )
        .assert_passes();
}

/// The same checks on datetime columns only, for the strict targets: the
/// rust target refuses Date-only values outright.
fn datetime_app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"tasks\", force: :cascade do |t|\n    t.datetime \"opened_at\"\n    t.datetime \"closed_at\"\n  end\nend\n",
        )
        .write(
            "app/models/task.rb",
            "class Task < ApplicationRecord\n  validates :opened_at, presence: true\n  validates :closed_at, absence: true\nend\n",
        )
}

/// The strict targets bake the same checks; no emit error.
#[test]
fn strict_targets_emit_temporal_presence_and_absence_without_errors() {
    use roundhouse::project::BuildTarget;
    for target in [BuildTarget::Rust, BuildTarget::Typescript, BuildTarget::Go, BuildTarget::Crystal] {
        let (_emitted, errors) = datetime_app().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
    }
}
