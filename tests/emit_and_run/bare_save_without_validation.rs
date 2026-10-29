//! A model's own `save!(validate: false)`, written bare, skips validation
//! as the receiver form does. lobsters' Comment#delete_for_user is
//! `self.is_deleted = true; save!(validate: false)`, and every comment
//! delete and undelete was `wrong number of arguments (given 1,
//! expected 0)`: the rewrite to `save_after_validation` only matched
//! `record.save!(validate: false)`.

use super::emit_and_run;

const ASSERTIONS: &str = r#"
a = Article.create!(title: "Bare save", body: "A body long enough for validation.")
c = Comment.create!(article: a, commenter: "x", body: "kept")
c.blank_out!
raise "saved: #{Comment.find(c.id).commenter.inspect}" unless Comment.find(c.id).commenter == ""
puts "bare save passed"
"#;

#[test]
fn a_bare_save_without_validation_skips_validation() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/comment.rb",
            "  validates :commenter, presence: true\n",
            "  validates :commenter, presence: true\n\n  def blank_out!\n    self.commenter = \"\"\n    save!(validate: false)\n  end\n",
        )
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("bare save passed"), "{}", run.stdout);
}

/// A bare `save(validate: false)` outside a model is another class's own
/// method, which may take the kwarg: it is left as written.
#[test]
fn a_bare_save_outside_a_model_is_left_alone() {
    let run = emit_and_run::real_blog()
        .write(
            "app/models/draft_box.rb",
            "class DraftBox\n  def save(validate: true)\n    validate ? \"checked\" : \"unchecked\"\n  end\n\n  def stash\n    save(validate: false)\n  end\nend\n",
        )
        .run_ruby("raise \"stash\" unless DraftBox.new.stash == \"unchecked\"\nputs \"service save passed\"\n");
    run.assert_passes();
    assert!(run.stdout.contains("service save passed"), "{}", run.stdout);
}
