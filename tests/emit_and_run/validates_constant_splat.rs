use super::emit_and_run;

/// `validates(*FIELDS, presence: true)` with `FIELDS = %i(name code)` in
/// the class body: Ruby expands the splat into the attribute names, so
/// the rule applies to each. The splat was a reported gap that refused
/// the call. A `%w` list and an `attr_accessor(*NOTES)` virtual expand
/// the same way. Answers are Rails 8.1.4's for the same model and records.
fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n    t.string \"code\"\n  end\nend\n",
        )
        .write(
            "app/models/widget.rb",
            r#"class Widget < ApplicationRecord
  FIELDS = %i(name code).freeze
  NOTES = %w[note memo].freeze

  attr_accessor(*NOTES)

  validates(*FIELDS, presence: true)
  validates(*NOTES, length: { maximum: 3 })
end
"#,
        )
}

#[test]
fn a_constant_splat_validates_each_named_attribute_like_rails() {
    app()
        .run_ruby(
            r##"rows = [["n", "c", "ab", "abc"], [nil, "c", nil, "abcd"], ["", nil, "abcd", nil]]
got = rows.map do |n, c, no, me|
  w = Widget.new
  w.name = n
  w.code = c
  w.note = no
  w.memo = me
  w.valid?
  w.errors.to_a
end
want = [
  [],
  ["Name can't be blank", "Memo is too long (maximum is 3 characters)"],
  ["Name can't be blank", "Code can't be blank", "Note is too long (maximum is 3 characters)"],
]
raise "errors: #{got.inspect}" unless got == want
puts "constant splat passed"
"##,
        )
        .assert_passes();
}

/// A splat this build cannot fold (a method call, a list with a
/// non-name element) stays the reported gap.
#[test]
fn an_unfoldable_splat_stays_a_reported_gap() {
    let warnings = emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n",
        )
        .write(
            "app/models/widget.rb",
            "class Widget < ApplicationRecord\n  MIXED = [:name, 1].freeze\n  def self.fields\n    %i[name]\n  end\n  validates(*fields, presence: true)\n  validates(*MIXED, presence: true)\nend\n",
        )
        .ingest_warnings();
    let refused = warnings.iter().filter(|w| w.contains("validates argument:")).count();
    assert_eq!(refused, 2, "{warnings:?}");
}
