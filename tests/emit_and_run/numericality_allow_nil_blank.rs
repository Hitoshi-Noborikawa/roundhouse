use super::emit_and_run;

/// `numericality:` with `allow_nil:` (in its own hash, or call-level) and
/// call-level `allow_blank:`. Rails skips the validator for a nil value
/// under allow_nil and for a blank one under allow_blank; the validator's
/// own hash wins over the call-level option (`rank` keeps rejecting nil).
/// These were reported gaps that refused the whole call. Answers are
/// Rails 8.1.4's for the same model and records. The records hold values
/// that need no type cast (Integers, Floats, nil, ""): a String number on
/// a typed column is #1158. On an untyped attribute the options stay a gap
/// (tests/numericality_allow_options.rs).
fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"policies\", force: :cascade do |t|\n    t.integer \"min_favs\"\n    t.integer \"age\"\n    t.integer \"rank\"\n  end\nend\n",
        )
        .write(
            "app/models/policy.rb",
            r#"class Policy < ApplicationRecord
  validates :min_favs, numericality: { greater_than_or_equal_to: 1, allow_nil: true }
  validates :age, numericality: { only_integer: true }, allow_blank: true
  validates :rank, numericality: { allow_nil: false }, allow_nil: true
end
"#,
        )
}

#[test]
fn numericality_skips_nil_and_blank_like_rails() {
    app()
        .run_ruby(
            r##"rows = [
  { min_favs: nil, age: nil, rank: 1 },
  { min_favs: 5, age: "", rank: 1 },
  { min_favs: 0, age: 3, rank: nil },
  { min_favs: 2, age: 5.5, rank: 1 },
]
got = rows.map do |attrs|
  p = Policy.new
  attrs.each { |k, v| p.public_send(:"#{k}=", v) }
  p.valid?
  p.errors.to_a
end
want = [[], [], ["Min favs must be greater than or equal to 1", "Rank is not a number"], ["Age must be an integer"]]
raise "policy errors: #{got.inspect}" unless got == want
puts "numericality allow_nil/allow_blank passed"
"##,
        )
        .assert_passes();
}

/// The skip lowers into the shared validation IR, so the strict targets
/// emit it too: a `nil?` test on their optional field, and no emit error.
#[test]
fn strict_targets_emit_the_skip_without_errors() {
    use roundhouse::project::BuildTarget;
    for target in [BuildTarget::Rust, BuildTarget::Typescript, BuildTarget::Go, BuildTarget::Crystal] {
        let (_emitted, errors) = app().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
    }
}
