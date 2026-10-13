//! `errors.add` works before the first `valid?`, as in Rails, where the
//! errors are created on first read; a later `valid?` starts them over.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn form_object(rules: &str) -> String {
    format!(
        r#"class Signup
  include ActiveModel::Validations
  attr_accessor :email
{rules}
  def flag
    errors.add(:email, "is taken")
    self
  end

  def email_problems = errors[:email]
end
"#
    )
}

const WITHOUT_RULES: &str = r#"
signup = Signup.new
raise "fresh" unless signup.email_problems.empty?
raise signup.email_problems.inspect unless signup.flag.email_problems == ["is taken"]
raise signup.errors.inspect unless signup.errors.to_a == ["Email is taken"]
raise "valid? starts over" unless signup.valid? && signup.email_problems.empty?
puts "without rules passed"
"#;

const WITH_RULES: &str = r#"
signup = Signup.new
raise signup.email_problems.inspect unless signup.flag.email_problems == ["is taken"]
raise "valid?" if signup.valid?
raise signup.email_problems.inspect unless signup.email_problems == ["can't be blank"]
puts "with rules passed"
"#;

#[test]
fn errors_add_before_valid_on_a_class_without_rules() {
    emit_and_run::real_blog().write("app/models/signup.rb", &form_object("")).run_ruby(WITHOUT_RULES).assert_passes();
}

#[test]
fn errors_add_before_valid_on_a_class_with_rules() {
    emit_and_run::real_blog()
        .write("app/models/signup.rb", &form_object("  validates :email, presence: true\n"))
        .run_ruby(WITH_RULES)
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn errors_add_before_valid_on_a_class_without_rules_on_spinel() {
    emit_and_run::real_blog().write("app/models/signup.rb", &form_object("")).run_spinel(WITHOUT_RULES).assert_passes();
}

/// An ActiveRecord model already has them from `Base#initialize`.
#[test]
fn errors_add_before_valid_on_an_active_record_model() {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def flag_title\n    errors.add(:title, :blank)\n    self\n  end\n\n  def title_problems = errors[:title]\n",
        )
        .run_ruby(r#"
article = Article.new(title: "A title", body: "A long enough body")
raise article.title_problems.inspect unless article.flag_title.title_problems == ["can't be blank"]
raise "valid? starts over" unless article.valid? && article.title_problems.empty?
puts "active record passed"
"#)
        .assert_passes();
}
