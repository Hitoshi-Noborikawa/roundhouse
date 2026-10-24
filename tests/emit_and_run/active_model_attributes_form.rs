//! A tableless `ActiveModel::Model` form that also includes
//! `ActiveModel::Attributes` (a class under `app/models`, so ingest
//! classifies it as a model). Rails 8.1.4: `new(count: "7")` assigns
//! through each attribute's casting writer, `:integer` `"7"` is 7,
//! `:string` 12 is `"12"`, a `:datetime` date string is that day at UTC
//! midnight, `:boolean` `"0"` is false; an `attr_accessor` beside them is
//! assigned as given; validations read the cast value.
//!
//! The model path's Attributes API cast only `:boolean`, and the include
//! was reported as unresolved.
use super::emit_and_run;

const FORM: &str = r#"class Form::Confirm
  include ActiveModel::Model
  include ActiveModel::Attributes

  attribute :count, :integer
  attribute :label, :string
  attribute :at, :datetime
  attribute :flag, :boolean
  attr_accessor :note

  validates :label, presence: true
end
"#;

const SCRIPT: &str = r##"
require_relative "app/models/form/confirm"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
f = Form::Confirm.new(count: "7", label: 12, at: "2026-10-14", flag: "0", note: "x")
check("integer", f.count, 7)
check("string", f.label, "12")
check("datetime", f.at, Time.utc(2026, 10, 14))
check("boolean", f.flag, false)
check("attr_accessor", f.note, "x")
f.count = "08"
check("writer casts", f.count, 8)
check("valid", f.valid?, true)
blank = Form::Confirm.new(label: "")
check("validation reads the cast value", blank.valid?, false)
puts "ActiveModel form attributes passed"
"##;

fn form() -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/models/form/confirm.rb", FORM)
}

#[test]
fn a_form_model_casts_its_attributes_on_ruby() {
    let run = form().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("ActiveModel form attributes passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_form_model_casts_its_attributes_on_spinel() {
    let run = form().run_spinel(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("ActiveModel form attributes passed"), "{}", run.stdout);
}

/// The include is the model path's to provide: no "includes unresolved".
#[test]
fn a_form_models_attributes_include_is_resolved() {
    let (_emitted, errors) = form().emit(roundhouse::project::BuildTarget::Ruby);
    assert!(!errors.iter().any(|e| e.contains("includes unresolved ActiveModel::Attributes")), "{errors:#?}");
    let warnings = form().ingest_warnings();
    assert!(!warnings.iter().any(|e| e.contains("includes unresolved ActiveModel::Attributes")), "{warnings:#?}");
}

const RECORD_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a = Article.new(title: "Typed", body: "A body long enough to validate.", rank: "7", tag: 12, flag: "0")
check("integer", a.rank, 7)
check("string", a.tag, "12")
check("boolean", a.flag, false)
a.rank = "08"
check("writer casts", a.rank, 8)
puts "ActiveRecord attributes passed"
"##;

/// An ActiveRecord model's `attribute :name, :type` (Rails 8.1.4) casts as
/// the ActiveModel form's does: `Record.new(count: "7").count` is 7 and
/// `label: 12` is `"12"`.
#[test]
fn a_record_casts_its_attribute_api_declarations_on_ruby() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n",
            "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n  attribute :rank, :integer\n  attribute :tag, :string\n  attribute :flag, :boolean\n",
        )
        .run_ruby(RECORD_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("ActiveRecord attributes passed"), "{}", run.stdout);
}
