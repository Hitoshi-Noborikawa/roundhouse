//! `validates_with` (#1348). activemodel 8.1.4 (oracle
//! `validates_with.rb`): the validator runs while the record validates
//! and its errors are the record's; `if:` (a lambda or a Symbol) and
//! `unless:` gate it, `on: :create` runs it on create and not on update,
//! and the declaration's own options reach the validator's `options`.
//!
//! A validator's `record.errors.add(:base, <computed message>)` is
//! grounded like a plain one: the message is evaluated once.
//!
//! The declaration was reported and dropped, and the validator class was
//! dropped as a subclass of an unported Rails base.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const SHOUT: &str = "class ShoutValidator < ActiveModel::Validator\n  def validate(record)\n    title = record.title.to_s\n    record.errors.add(:title, \"is shouty\") if !title.empty? && title == title.upcase\n  end\nend\n";
const LIMIT: &str = "class LimitValidator < ActiveModel::Validator\n  def validate(record)\n    record.errors.add(:base, \"body too long\") if record.body.to_s.length > options[:max]\n  end\nend\n";
const COMPUTED: &str = "class ComputedValidator < ActiveModel::Validator\n  def validate(record)\n    record.errors.add(:base, message_for(record)) if record.title.to_s == \"computed\"\n  end\n\n  def message_for(record)\n    \"title \" + record.title.to_s + \" is reserved\"\n  end\nend\n";
const FRESH: &str = "class FreshValidator < ActiveModel::Validator\n  def validate(record)\n    record.errors.add(:title, \"is taken\") if record.title.to_s == \"taken\"\n  end\nend\n";

const ARTICLE: &str = "  has_many :comments, dependent: :destroy\n  validates_with ShoutValidator, if: -> { title.to_s != \"SKIP\" }\n  validates_with LimitValidator, max: 60, unless: :exempt?\n  validates_with FreshValidator, on: :create\n  validates_with ComputedValidator\n\n  def exempt?\n    title.to_s == \"exempt\"\n  end\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
a = Article.new(title: "LOUD", body: body)
check("runs, error on the attribute", [a.valid?, a.errors.include?("Title is shouty")], [false, true])
check("if: lambda false skips", Article.new(title: "SKIP", body: body).valid?, true)
long = "x" * 70
b = Article.new(title: "quiet", body: long)
check("a custom option reaches the validator", [b.valid?, b.errors.include?("body too long")], [false, true])
check("unless: Symbol skips", Article.new(title: "exempt", body: long).valid?, true)
c = Article.new(title: "taken", body: body)
check("on: :create runs on create", c.valid?, false)
d = Article.create!(title: "fine", body: body)
d.title = "taken"
check("on: :create skipped on update", d.valid?, true)
e = Article.new(title: "computed", body: body)
check("a computed message", [e.valid?, e.errors.include?("title computed is reserved")], [false, true])
puts "validates_with passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", ARTICLE)
        .write("app/validators/shout_validator.rb", SHOUT)
        .write("app/validators/limit_validator.rb", LIMIT)
        .write("app/validators/fresh_validator.rb", FRESH)
        .write("app/validators/computed_validator.rb", COMPUTED)
}

#[test]
fn validates_with_runs_the_validator_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("validates_with passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn validates_with_runs_the_validator_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("validates_with passed"), "{}", run.stdout);
}
