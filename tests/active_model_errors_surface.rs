//! A class that includes `ActiveModel::Validations` or `ActiveModel::Model`
//! has `valid?`, `errors` and the error details even when it declares no
//! `validates` rule: a form object that adds its own errors in `save` runs
//! as Rails runs it.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

fn form_object(module: &str) -> String {
    format!(
        r#"class Signup
  include ActiveModel::{module}

  def save
    return false unless valid?
    errors.add(:email, "is taken")
    false
  end

  def email_problems = errors[:email]
end
"#
    )
}

const SCRIPT: &str = r#"
signup = Signup.new
raise "save" unless signup.save == false
raise signup.email_problems.inspect unless signup.email_problems == ["is taken"]
raise signup.errors.inspect unless signup.errors.to_a == ["Email is taken"]
raise "valid? clears" unless signup.valid? && signup.email_problems.empty?
puts "errors surface passed"
"#;

#[test]
fn a_validations_class_without_rules_has_its_errors() {
    emit_and_run::real_blog().write("app/models/signup.rb", &form_object("Validations")).run_ruby(SCRIPT).assert_passes();
}

#[test]
fn a_model_class_without_rules_has_its_errors() {
    emit_and_run::real_blog().write("app/models/signup.rb", &form_object("Model")).run_ruby(SCRIPT).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_validations_class_without_rules_has_its_errors_on_spinel() {
    emit_and_run::real_blog().write("app/models/signup.rb", &form_object("Validations")).run_spinel(SCRIPT).assert_passes();
}
