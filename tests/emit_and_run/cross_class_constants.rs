use super::emit_and_run;

/// Validations that name another owner's constants: a class's Integer
/// and its Regexp (interpolating one of its Strings), and a module's
/// list and `Regexp.union`. These were reported gaps that refused the
/// whole call. Answers are Rails 8.1.4's for the same models and records.
fn app() -> emit_and_run::Overlay {
    owners(
        r##"class Rule < ApplicationRecord
  TEXT_SIZE_LIMIT = 5
  NAME_PAT = "[a-z]+"
  NAME_RE = /\A#{NAME_PAT}\z/i
end
"##,
        "module Kinds\n  CATEGORIES = %w(account content).freeze\n  SLUGS = Regexp.union(%w(a.b c+d))\nend\n",
    )
}

fn owners(rule: &str, kinds: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"rules\", force: :cascade do |t|\n    t.string \"text\"\n  end\n  create_table \"notes\", force: :cascade do |t|\n    t.string \"text\"\n    t.string \"name\"\n    t.string \"category\"\n    t.string \"slug\"\n  end\nend\n",
        )
        .write("app/models/rule.rb", rule)
        .write("app/models/kinds.rb", kinds)
        .write(
            "app/models/note.rb",
            r#"class Note < ApplicationRecord
  validates :text, length: { maximum: Rule::TEXT_SIZE_LIMIT }
  validates :name, format: { with: Rule::NAME_RE }
  validates :category, inclusion: Kinds::CATEGORIES
  validates :slug, format: { with: Kinds::SLUGS }
end
"#,
        )
}

#[test]
fn another_owners_constants_validate_like_rails() {
    app()
        .run_ruby(
            r##"rows = [
  { text: "abc", name: "Ab", category: "account", slug: "a.b" },
  { text: "abcdef", name: "a1", category: "other", slug: "axb" },
  { text: "", name: "zz", category: "content", slug: "c+d" },
]
got = rows.map do |attrs|
  n = Note.new
  attrs.each { |k, v| n.public_send(:"#{k}=", v) }
  n.valid?
  n.errors.to_a
end
want = [[], ["Text is too long (maximum is 5 characters)", "Name is invalid", "Category is not included in the list", "Slug is invalid"], []]
raise "note errors: #{got.inspect}" unless got == want
puts "cross-class constants passed"
"##,
        )
        .assert_passes();
}

/// The strict targets get the same folded values, with no emit error.
/// The owners spell each value as a literal here: a strict target does
/// not emit `Regexp.union` or an interpolated Regexp in the owner's own
/// body (a separate, reported gap), whatever the validation reads.
#[test]
fn strict_targets_emit_the_folded_checks_without_errors() {
    use roundhouse::project::BuildTarget;
    let app = || {
        owners(
            "class Rule < ApplicationRecord\n  TEXT_SIZE_LIMIT = 5\n  NAME_RE = /\\A[a-z]+\\z/i\nend\n",
            "module Kinds\n  CATEGORIES = %w(account content).freeze\n  SLUGS = /a\\.b|c\\+d/\nend\n",
        )
    };
    for target in [BuildTarget::Rust, BuildTarget::Typescript, BuildTarget::Go, BuildTarget::Crystal] {
        let (_, errors) = app().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
    }
}
