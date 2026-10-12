//! A concern's `included do` block runs with `self` bound to each
//! includer, so a declaration guarded on the includer
//! (`has_many :x, ... if self == Article`) belongs to the includers the
//! guard admits. The statement used to come back from ingest as an
//! unknown `if` and was dropped from every includer, with no diagnostic:
//! the reader was simply missing at run time.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const CONCERN: &str = r#"
module Remarkable
  extend ActiveSupport::Concern

  included do
    has_many :remarks, class_name: "Comment", dependent: :destroy if self == Article
    has_many :sibling_remarks, class_name: "Comment", foreign_key: :article_id, primary_key: :article_id unless self == Article
    scope :titled, ->(t) { where(title: t) } if in?([Article])
  end
end
"#;

#[test]
fn includer_guarded_declarations_reach_the_includers_they_name() {
    emit_and_run::real_blog()
        .write("app/models/concerns/remarkable.rb", CONCERN)
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  include Remarkable\n",
        )
        .edit(
            "app/models/comment.rb",
            "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  include Remarkable\n",
        )
        .run_ruby(
            r#"
article = Article.create!(title: "Widget", body: "A sufficiently long body")
first = Comment.create!(article_id: article.id, commenter: "Gadget", body: "first")
Comment.create!(article_id: article.id, commenter: "Sprocket", body: "second")

raise "if self == Article: #{article.remarks.to_a.size}" unless article.remarks.to_a.size == 2
raise "scope if in?([Article])" unless Article.titled("Widget").to_a.size == 1
raise "unless self == Article" unless first.sibling_remarks.to_a.size == 2

raise "Comment must not get remarks" if first.respond_to?(:remarks)
raise "Article must not get sibling_remarks" if article.respond_to?(:sibling_remarks)
raise "Comment must not get titled" if Comment.respond_to?(:titled)
puts "ok"
"#,
        )
        .assert_passes();
}

/// A concern reopened in a second file: both files' `included do`
/// blocks run for the includer, so both files' guarded declarations
/// reach it (the second file's used to replace the first's).
#[test]
fn a_concern_reopened_in_two_files_keeps_both_files_guarded_items() {
    emit_and_run::real_blog()
        .write(
            "app/models/concerns/remarkable.rb",
            r#"
module Remarkable
  extend ActiveSupport::Concern

  included do
    has_many :remarks, class_name: "Comment", dependent: :destroy if self == Article
    validates :title, length: { minimum: 2 }
  end
end
"#,
        )
        .write(
            "app/models/concerns/remarkable_reopened.rb",
            r#"
module Remarkable
  included do
    has_many :more_remarks, class_name: "Comment" if self == Article
    validates :title, length: { maximum: 40 }
  end
end
"#,
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  include Remarkable\n",
        )
        .run_ruby(
            r#"
article = Article.create!(title: "Widget", body: "A sufficiently long body")
Comment.create!(article_id: article.id, commenter: "Gadget", body: "first")
raise "first file's guarded has_many" unless article.respond_to?(:remarks) && article.remarks.to_a.size == 1
raise "second file's guarded has_many" unless article.respond_to?(:more_remarks) && article.more_remarks.to_a.size == 1
raise "first file's validation" if Article.new(title: "W", body: "A sufficiently long body").valid?
raise "second file's validation" if Article.new(title: "W" * 41, body: "A sufficiently long body").valid?
puts "ok"
"#,
        )
        .assert_passes();
}
