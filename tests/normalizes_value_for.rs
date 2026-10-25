//! `normalizes … apply_to_nil: true` and `normalize_value_for`
//! (activerecord 8.1.4, oracle `normalizes.rb`): with `apply_to_nil`,
//! an assigned nil is normalized too (`->(t) { t.to_s }` makes it "");
//! `Model.normalize_value_for(:attr, value)` applies the attribute's
//! normalization, called on the model, from its class methods and
//! scopes, or as `self.class.` in an instance method.
//!
//! The value is cast by the column's type first (`normalizes2.rb`):
//! `title: 123` normalizes "123", and an Integer column's "21" is 21.
//!
//! `apply_to_nil: true` left the declaration unapplied, and
//! `normalize_value_for` had no lowering ("no known method").

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const ARTICLE: &str = "  has_many :comments, dependent: :destroy\n  normalizes :score, with: ->(score) { score * 2 }\n  normalizes :title, with: ->(title) { title.strip.delete_prefix(\"@\") }\n  normalizes :body, with: ->(body) { body.to_s.squish }, apply_to_nil: true\n\n  scope :titled, ->(raw) { where(title: normalize_value_for(:title, raw)) }\n\n  def self.normalized_title(raw)\n    normalize_value_for(:title, raw)\n  end\n\n  def same_title?(raw)\n    self.class.normalize_value_for(:title, raw) == title\n  end\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
check("assignment", Article.new(title: "  @hello ").title, "hello")
check("apply_to_nil normalizes nil", Article.new(body: nil).body, "")
check("apply_to_nil keeps squishing", Article.new(body: "  a   b ").body, "a b")
check("from a class method", Article.normalized_title(" @y "), "y")
a = Article.create!(title: "z", body: "A body long enough to validate.")
check("from an instance method", a.same_title?(" @z "), true)
check("in a scope", Article.titled(" @z ").count, 1)
puts "normalizes passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", "    t.string \"title\"\n", "    t.string \"title\"\n    t.integer \"score\"\n")
        .edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", ARTICLE)
}

// A non-String value, which a typed (Spinel) program doesn't pass: CRuby only.
const CAST_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
check("an Integer to a String column", Article.new(title: 123).title, "123")
check("normalize_value_for casts", Article.normalized_title(123), "123")
check("a String to an Integer column", Article.new(score: "21").score, 42)
a = Article.create!(title: "123", body: "A body long enough to validate.")
check("a finder keyword casts", Article.titled(123).count, 1)
puts "cast passed"
"##;

#[test]
fn the_value_is_cast_by_the_column_type_first_on_ruby() {
    let run = app().run_ruby(CAST_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("cast passed"), "{}", run.stdout);
}

#[test]
fn apply_to_nil_and_normalize_value_for_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("normalizes passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn apply_to_nil_and_normalize_value_for_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("normalizes passed"), "{}", run.stdout);
}
