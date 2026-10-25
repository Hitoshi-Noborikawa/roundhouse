//! A nested child's required `belongs_to` where Rails infers no inverse
//! (activerecord 8.1.4, oracle `inverse.rb`). Rails infers an inverse only
//! when neither side writes `foreign_key:` (or `through:`), or when one
//! says `inverse_of:`; without one, a child built on a new owner has no
//! owner when it's validated, so "… must exist" fails the owner's save.
//! A saved owner's children have its key and pass.
//!
//! The nested check always took the child's owner as set, skipping its
//! "must exist", so such an owner saved where Rails rejects it.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
body = "A body long enough to validate."
a = Article.new(title: "t", body: body, comments_attributes: [{ commenter: "c", body: "one" }])
check("no inverse: the new owner's save fails", [a.save, a.errors, Comment.count], [false, ["Comments article must exist"], 0])
b = Article.create!(title: "u", body: body)
check("a saved owner's child has its key", [b.update(comments_attributes: [{ commenter: "c", body: "two" }]), Comment.where(article_id: b.id).count], [true, 1])
puts "nested inverse passed"
"##;

fn app(belongs_to: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy\n  accepts_nested_attributes_for :comments\n",
        )
        .edit("app/models/comment.rb", "  belongs_to :article\n", belongs_to)
}

#[test]
fn a_foreign_key_leaves_a_nested_child_without_its_owner_on_ruby() {
    let run = app("  belongs_to :article, class_name: \"Article\", foreign_key: \"article_id\"\n").run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested inverse passed"), "{}", run.stdout);
}

const INVERSE_SCRIPT: &str = r##"
a = Article.new(title: "t", body: "A body long enough to validate.", comments_attributes: [{ commenter: "c", body: "one" }])
raise "inverse_of: #{[a.save, a.errors].inspect}" unless a.save && Comment.count == 1
puts "inverse_of passed"
"##;

#[test]
fn an_inverse_of_keeps_the_owner_set_on_ruby() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy\n",
            "  has_many :comments, dependent: :destroy, inverse_of: :article\n  accepts_nested_attributes_for :comments\n",
        )
        .edit("app/models/comment.rb", "  belongs_to :article\n", "  belongs_to :article, foreign_key: \"article_id\", inverse_of: :comments\n")
        .run_ruby(INVERSE_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("inverse_of passed"), "{}", run.stdout);
}
