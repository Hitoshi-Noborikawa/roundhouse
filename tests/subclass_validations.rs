//! An abstract parent's validations (#1360, case 1).
//! activerecord 8.1.4 (oracle `oracle.rb`, which runs these same classes
//! and script): a subclass record runs the parent's `validate`
//! and `validates`, before its own.
//!
//! The subclass's own synthesized `validate` covered only its own
//! declarations, so the parent's checks never ran on it, and nothing
//! reported it.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const TITLED_RECORD: &str = "class TitledRecord < ApplicationRecord\n  self.abstract_class = true\n\n  validate :title_not_reserved\n  validates :title, length: { maximum: 20 }\n\n  def title_not_reserved\n    errors.add(:title, \"is reserved\") if title == \"admin\"\n  end\nend\n";
const ARTICLE: &str = "class Article < TitledRecord\n  has_many :comments, dependent: :destroy\n\n  validates :title, presence: true\n  validates :body, presence: true, length: { minimum: 10 }\n  validate :body_not_placeholder\n\n  def body_not_placeholder\n    errors.add(:body, \"is placeholder\") if body == \"lorem ipsum dolor\"\n  end\nend\n";
const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
# Rails' errors answer `full_messages`; this runtime's errors are the messages.
messages = ->(record) { record.valid?; record.errors.respond_to?(:full_messages) ? record.errors.full_messages : record.errors }
body = "A body long enough to validate."
check("the parent's validate", messages.(Article.new(title: "admin", body: body)), ["Title is reserved"])
check("the parent's validates", messages.(Article.new(title: "t" * 21, body: body)), ["Title is too long (maximum is 20 characters)"])
check("the subclass's own validate", messages.(Article.new(title: "ok", body: "lorem ipsum dolor")), ["Body is placeholder"])
check("both, the parent's first", messages.(Article.new(title: "admin", body: "lorem ipsum dolor")), ["Title is reserved", "Body is placeholder"])
check("a valid record", Article.new(title: "ok", body: body).valid?, true)
puts "subclass validations passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("app/models/titled_record.rb", TITLED_RECORD)
        .write("app/models/article.rb", ARTICLE)
}

#[test]
fn a_subclass_runs_its_abstract_parents_validations_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("subclass validations passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_subclass_runs_its_abstract_parents_validations_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("subclass validations passed"), "{}", run.stdout);
}
