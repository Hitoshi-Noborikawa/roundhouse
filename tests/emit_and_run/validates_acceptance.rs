use super::emit_and_run;

/// `validates :attr, acceptance:`. Rails' `AcceptanceValidator` adds
/// "must be accepted" unless `Array(accept).include?(value)`, with
/// `accept:` defaulting to `["1", true]` and `allow_nil:` to `true`. A
/// Boolean column is cast before the check, so `"1"` and `"yes"` are
/// `true` there and `""` is nil. Answers are Rails 8.1.4's for the same
/// model and records.
fn members() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"members\", force: :cascade do |t|\n    t.string \"name\"\n    t.boolean \"active\"\n  end\nend\n",
        )
        .write(
            "app/models/member.rb",
            r#"class Member < ApplicationRecord
  attr_accessor :agreement, :terms

  validates :agreement, acceptance: { allow_nil: false, accept: [true, 'true', '1'] }, on: :create
  validates :terms, acceptance: true
  validates :active, acceptance: true
end
"#,
        )
}

const MEMBERS: &str = r##"def errs(agreement, terms, active)
  m = Member.new
  m.name = "x"
  m.agreement = agreement
  m.terms = terms
  m.active = active
  m.valid?
  m.errors.to_a
end
got = [
  errs("1", nil, nil),
  errs(nil, "1", "1"),
  errs(true, true, true),
  errs("true", "true", "0"),
  errs("yes", "0", "yes"),
  errs(false, false, false),
]
want = [
  [],
  ["Agreement must be accepted"],
  [],
  ["Terms must be accepted", "Active must be accepted"],
  ["Agreement must be accepted", "Terms must be accepted"],
  ["Agreement must be accepted", "Terms must be accepted", "Active must be accepted"],
]
raise "member errors: #{got.inspect}" unless got == want
m = Member.new
m.name = "x"
m.agreement = "1"
m.save!
m.agreement = nil
raise "on: :create checked an update: #{m.errors.to_a.inspect}" unless m.valid?
puts "acceptance passed"
"##;

#[test]
fn acceptance_matches_rails() {
    let run = members().run_ruby(MEMBERS);
    run.assert_passes();
    assert!(run.stdout.contains("acceptance passed"), "{}", run.stdout);
}

/// The strict targets emit the check with no emit error. A Boolean
/// column compares only the accepted Booleans.
#[test]
fn strict_targets_emit_the_acceptance_check_without_errors() {
    use roundhouse::project::BuildTarget;
    for target in [BuildTarget::Rust, BuildTarget::Typescript, BuildTarget::Go, BuildTarget::Crystal] {
        let (emitted, errors) = members().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
        if target == BuildTarget::Rust {
            let member = std::fs::read_to_string(emitted.join("src/models/member.rs")).expect("member.rs");
            let validate = &member[member.find("pub fn validate(").expect("validate")..];
            let validate = &validate[..validate.find("\n    }\n").unwrap_or(validate.len())];
            let active = validate.lines().find(|l| l.contains("self.active")).expect("the active check");
            assert!(active.contains("== true"), "{active}");
            assert!(!active.contains("\"1\""), "{active}");
        }
    }
}

/// A name declared only by `attribute :consent, :boolean` is a reported
/// gap too: its writer turns nil and `""` into `false`, where Rails keeps
/// nil and skips the check (Rails 8.1.4 accepts both here).
#[test]
fn acceptance_on_an_attribute_api_name_is_a_reported_gap() {
    let warnings = emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"signups\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n",
        )
        .write(
            "app/models/signup.rb",
            "class Signup < ApplicationRecord\n  attribute :consent, :boolean\n  validates :consent, acceptance: true\nend\n",
        )
        .ingest_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("validates :consent, acceptance:") && w.contains("does not cast")),
        "{warnings:?}"
    );
}

/// An attribute the model does not declare is a reported gap that
/// refuses the whole call: Rails defines a virtual accessor for it, which
/// this build does not synthesize. The `presence:` beside it goes too.
#[test]
fn acceptance_on_an_undeclared_attribute_is_a_reported_gap() {
    let app = || emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"members\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n",
        )
        .write(
            "app/models/member.rb",
            "class Member < ApplicationRecord\n  validates :agreement, acceptance: true, presence: true\nend\n",
        );
    let warnings = app().ingest_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("validates :agreement, acceptance:") && w.contains("does not declare")),
        "{warnings:?}"
    );
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    let member = std::fs::read_to_string(emitted.join("app/models/member.rb")).expect("member.rb");
    assert!(!member.contains("must be accepted") && !member.contains("can't be blank"), "{member}");
}
