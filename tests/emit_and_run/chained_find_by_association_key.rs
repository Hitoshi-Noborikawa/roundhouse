//! A condition hash on a terminal at the end of a `where` chain keys off
//! the association as `where`'s own does. lobsters' Comment#
//! breaks_speed_limit? reads `Comment.where("created_at >= ?", t)
//! .find_by(user: user, thread_id: …)`, which emitted `comments."user" = ?`
//! — no such column — and every reply to a comment was a 500.

use super::emit_and_run;

const ASSERTIONS: &str = r#"
a = Article.create!(title: "Chained", body: "A body long enough for validation.")
b = Article.create!(title: "Other", body: "A body long enough for validation.")
Comment.create!(article: a, commenter: "x", body: "first")
got = [Comment.recent_on(a)&.body, Comment.recent_on(b), Comment.any_recent_on?(a), Comment.any_recent_on?(b)]
raise "chained: #{got.inspect}" unless got == ["first", nil, true, false]
puts "chained find_by passed"
"#;

#[test]
fn a_chained_terminal_keys_its_condition_hash_off_the_association() {
    let run = emit_and_run::real_blog()
        .edit(
            "app/models/comment.rb",
            "  validates :commenter, presence: true\n",
            "  validates :commenter, presence: true\n\n  def self.recent_on(a)\n    Comment.where(\"created_at >= ?\", 1.day.ago).find_by(article: a, commenter: \"x\")\n  end\n\n  def self.any_recent_on?(a)\n    Comment.where(\"created_at >= ?\", 1.day.ago).exists?(article: a)\n  end\n",
        )
        .run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("chained find_by passed"), "{}", run.stdout);
}
