//! `errors.add` with an attribute or options known only at run time
//! (activerecord 8.1.4, oracle `errors_add.rb`): a validator's
//! `record.errors.add(attribute, :invalid)` gives "Body is invalid" (the
//! model's translated name when the locale has one: "Heading is invalid"),
//! `count:` and `value:` interpolate into the locale's message (and
//! `count:` picks its plural form), and a String message is used as
//! given; on `:base` the message stands alone.
//!
//! These sites were reported as `errors.add` "left as dynamic dispatch",
//! and the emitted call raised NoMethodError (`add` on the Array the
//! errors are) when it ran.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const LOCALE: &str = "en:\n  hello: \"Hello world\"\n  activerecord:\n    attributes:\n      article:\n        title: \"Heading\"\n    errors:\n      messages:\n        too_many_words: \"has more than %{count} words\"\n        taken_value: \"%{value} is taken\"\n        one_or_more:\n          one: \"needs one item\"\n          other: \"needs %{count} items\"\n";

const CHECKER: &str = "class Checker\n  def self.invalid(record, attribute)\n    record.errors.add(attribute, :invalid)\n  end\n\n  def self.blank(record, attribute)\n    record.errors.add(attribute, :blank)\n  end\n\n  def self.words(record, attribute, limit)\n    record.errors.add(attribute, :too_many_words, count: limit)\n  end\n\n  def self.taken(record, attribute, value)\n    record.errors.add(attribute, :taken_value, value: value)\n  end\n\n  def self.items(record, attribute, n)\n    record.errors.add(attribute, :one_or_more, count: n)\n  end\n\n  def self.wrong(record, attribute)\n    record.errors.add(attribute, \"looks wrong\")\n  end\nend\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
def fresh = Article.new(title: "t", body: "A body long enough to validate.")
a = fresh; Checker.invalid(a, :body)
check("a variable attribute, :invalid", [a.errors, a.error_details.map { |d| [d.attribute, d.kind] }], [["Body is invalid"], [["body", "invalid"]]])
a = fresh; Checker.invalid(a, :title)
check("the model's translated attribute name", a.errors, ["Heading is invalid"])
a = fresh; Checker.blank(a, :body)
check(":blank", a.errors, ["Body can't be blank"])
a = fresh; Checker.words(a, :body, 3)
check("count: interpolated", a.errors, ["Body has more than 3 words"])
a = fresh; Checker.taken(a, :body, "abc")
check("value: interpolated", a.errors, ["Body abc is taken"])
a = fresh; Checker.items(a, :body, 1)
check("count: 1 picks one", a.errors, ["Body needs one item"])
a = fresh; Checker.items(a, :body, 2)
check("count: 2 picks other", a.errors, ["Body needs 2 items"])
a = fresh; Checker.wrong(a, :body)
check("a String message", a.errors, ["Body looks wrong"])
a = fresh; Checker.invalid(a, :base)
check(":base stands alone", a.errors, ["is invalid"])
puts "dynamic errors.add passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .write("config/locales/en.yml", LOCALE)
        .write("app/models/checker.rb", CHECKER)
}

#[test]
fn errors_add_with_a_runtime_attribute_or_options_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("dynamic errors.add passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_add_with_a_runtime_attribute_or_options_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("dynamic errors.add passed"), "{}", run.stdout);
}

// `errors.add(:base, <method call>)`: a literal `:base` with a computed
// message is grounded already (the message is evaluated once into a
// local); kept here as the shape a large app's model uses.
const BASE_SCRIPT: &str = r##"
a = Article.new(title: "admin", body: "A body long enough to validate.")
got = [a.valid?, a.errors, a.error_details.map { |d| [d.attribute, d.kind] }]
want = [false, ["the title admin is reserved"], [["base", "the title admin is reserved"]]]
raise "base: #{got.inspect}" unless got == want
puts "base message passed"
"##;

#[test]
fn errors_add_on_base_with_a_method_call_message_on_ruby() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  validate :not_reserved\n\n  def not_reserved\n    errors.add(:base, reserved_message) if title == \"admin\"\n  end\n\n  def reserved_message\n    \"the title \" + title.to_s + \" is reserved\"\n  end\n",
        )
        .run_ruby(BASE_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("base message passed"), "{}", run.stdout);
}
