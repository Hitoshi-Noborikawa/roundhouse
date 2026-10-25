//! A nested record's validation reaching its owner (#1335, part 4).
//! activerecord 8.1.4 (oracle `nested_validation.rb`):
//! - `accepts_nested_attributes_for` makes the association autosave: a
//!   loaded child edited to invalid fails the owner's `save`, its error
//!   under the association ("Comments body can't be blank"), with nothing
//!   written; one edited validly is saved with the owner. `has_many …,
//!   autosave: true` does the same without nested attributes.
//! - The association's check runs where it's declared, so its errors list
//!   before a later `validates`'s.
//! - Two children with the same error add it to the owner once.
//!
//! Before: an association reader re-queried on every call, so the owner
//! never saw an edited child and saved without it; the check ran after
//! every `validates`; and each child added its own copy of an error.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const BODY: &str = "A body long enough to validate.";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
both = Article.new(title: "", body: body, comments_attributes: [{ commenter: "c", body: "" }])
both.valid?
check("the association's errors list first", both.errors, ["Comments body can't be blank", "Title can't be blank"])
twice = Article.new(title: "t", body: body, comments_attributes: [{ commenter: "c", body: "" }, { commenter: "d", body: "" }])
twice.valid?
check("the same error once", twice.errors, ["Comments body can't be blank"])
check("one detail", twice.error_details.map(&:attribute), ["comments.body"])
a = Article.create!(title: "t", body: body, comments_attributes: [{ commenter: "c", body: "one" }])
id = Comment.where(article_id: a.id).first.id
q = Article.find(a.id)
q.comments.to_a.first.body = ""
check("a loaded child edited to invalid fails the owner", q.save, false)
check("its error under the association", q.errors, ["Comments body can't be blank"])
check("nothing written", Comment.find(id).body, "one")
r = Article.find(a.id)
r.comments.to_a.first.body = "two"
check("one edited validly saves", r.save, true)
check("with the owner", Comment.find(id).body, "two")
check("an unloaded association is left alone", Article.find(a.id).save, true)
puts "nested validation passed"
"##;

fn nested_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n",
        "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n  accepts_nested_attributes_for :comments\n",
    )
}

#[test]
fn a_nested_childs_validation_reaches_its_owner_on_ruby() {
    let run = nested_app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested validation passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_nested_childs_validation_reaches_its_owner_on_spinel() {
    let run = nested_app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested validation passed"), "{}", run.stdout);
}

const AUTOSAVE_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
a = Article.create!(title: "t", body: "BODY")
c = Comment.create!(article_id: a.id, commenter: "c", body: "one")
q = Article.find(a.id)
q.comments.to_a.first.body = ""
check("autosave: an invalid loaded child fails the owner", q.save, false)
check("its error", q.errors, ["Comments body can't be blank"])
check("nothing written", Comment.find(c.id).body, "one")
r = Article.find(a.id)
r.comments.to_a.first.body = "two"
r.save
check("autosave: a valid edit is saved", Comment.find(c.id).body, "two")
puts "autosave passed"
"##;

#[test]
fn has_many_autosave_validates_and_saves_loaded_children_on_ruby() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy, autosave: true\n",
        )
        .run_ruby(&AUTOSAVE_SCRIPT.replace("BODY", BODY));
    run.assert_passes();
    assert!(run.stdout.contains("autosave passed"), "{}", run.stdout);
}
