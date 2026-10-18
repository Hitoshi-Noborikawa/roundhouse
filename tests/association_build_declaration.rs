//! Emitted-program regression (kept out of tests/emit_and_run.rs so
//! concurrent appends there do not conflict). Same harness, plus the
//! declaration lookup the rewrite rests on.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const CREATED_UNDER_THE_ARTICLE: (&str, &str) = (
    "    assert_redirected_to article_url(@article)\n  end\n\n  test \"should not create comment with invalid params\" do",
    "    assert_redirected_to article_url(@article)\n    assert_equal @article.id, Comment.last.article_id\n    assert_equal \"A test comment.\", Comment.last.body\n  end\n\n  test \"should not create comment with invalid params\" do",
);

/// `has_many :remarks, class_name: "Comment"` — the parent-ivar
/// rewrite named the class after the association (`Remark`), which
/// does not exist. It builds and finds through the declared class.
#[test]
fn a_nested_build_and_find_through_class_name_use_the_declared_class() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Comment\"",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.find(params.expect(:id))",
            "@article.remarks.find(params.expect(:id))",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// An ordering scope changes neither what is built nor which row an id
/// finds, so the rewrite still expands it through the declared class.
#[test]
fn a_nested_build_and_find_through_an_ordering_scope_use_the_declared_class() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, -> { order(:id) }, class_name: \"Comment\"",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.find(params.expect(:id))",
            "@article.remarks.find(params.expect(:id))",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// A scope that changes the row set: Rails builds with its `where`
/// values (`commenter: "amy"`), which `<Target>.new(fk => parent.id)`
/// would drop without a word. The rewrite declines it, so the call
/// keeps its source shape instead of building a different record.
#[test]
fn a_nested_build_through_a_row_changing_scope_is_not_expanded() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, -> { where(commenter: \"amy\") }, class_name: \"Comment\"",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@article.remarks.build("), "{controller}");
    assert!(!controller.contains("Comment.from_params"), "{controller}");
}

/// A parent ivar not named after its class (`@post` holding an
/// Article): the foreign key is the declared `article_id`, not one
/// derived from the ivar's name (`post_id`).
#[test]
fn a_nested_build_through_an_ivar_not_named_after_the_owner_uses_the_declared_foreign_key() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.build(comment_params)",
            "    @post = @article\n    @comment = @post.comments.build(comment_params)",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// `@post` holding an Article while a `Post` model also declares
/// `comments` (keyed on `post_id`): the declaration comes from the
/// ivar's analyzed class, not from its name, so the comment is still
/// built under `article_id`.
#[test]
fn a_nested_build_through_an_ivar_named_after_another_declarer_uses_the_held_class() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.integer \"article_id\", null: false\n",
            "    t.integer \"article_id\", null: false\n    t.integer \"post_id\"\n",
        )
        .edit(
            "db/schema.rb",
            "  add_foreign_key \"comments\", \"articles\"",
            "  create_table \"posts\", force: :cascade do |t|\n    t.string \"title\"\n  end\n\n  add_foreign_key \"comments\", \"articles\"",
        )
        .write("app/models/post.rb", "class Post < ApplicationRecord\n  has_many :comments\nend\n")
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.build(comment_params)",
            "    @post = @article\n    @comment = @post.comments.build(comment_params)",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// The association registry over models ingested from `sources`, as
/// the controller lowering builds it.
fn registry(schema: &str, sources: &[(&str, &str)]) -> roundhouse::lower::scope_chain::AssocRegistry {
    let schema = roundhouse::ingest::ingest_schema(schema.as_bytes(), "db/schema.rb").expect("ingest schema");
    let models: Vec<_> = sources
        .iter()
        .map(|(src, path)| {
            roundhouse::ingest::ingest_model(src.as_bytes(), path, &schema, &Default::default())
                .expect("ingest model")
                .expect("model recognized")
        })
        .collect();
    roundhouse::lower::scope_chain::build_assoc_registry(&models)
}

/// The foreign key `<owner>.comments.<call>` expands with, `None` when
/// the rewrite declines it.
fn comments_key(
    reg: &roundhouse::lower::scope_chain::AssocRegistry,
    owner: Option<&str>,
    call: &str,
) -> Option<String> {
    use roundhouse::ident::{ClassId, Symbol};
    use roundhouse::lower::scope_chain::AssocBuildTarget;

    let owner = owner.map(|o| ClassId(Symbol::from(o)));
    match reg.build_target(owner.as_ref(), &Symbol::from("comments"), &Symbol::from(call)) {
        AssocBuildTarget::Declared { target, foreign_key } => {
            assert_eq!(target.0.as_str(), "Comment");
            Some(foreign_key.as_str().to_string())
        }
        AssocBuildTarget::Declined => None,
        AssocBuildTarget::Undeclared => panic!("comments is declared"),
    }
}

/// The parent's class picks between declarations; without one, the
/// name alone does not. A subclass finds the declaration it inherits.
#[test]
fn the_declaration_is_resolved_by_the_parent_class_not_the_ivar_name() {
    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
    t.string "type"
  end
  create_table "posts", force: :cascade do |t|
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
    t.integer "post_id"
  end
end
"#,
        &[
            ("class Article < ApplicationRecord\n  has_many :comments\nend\n", "app/models/article.rb"),
            ("class Post < ApplicationRecord\n  has_many :comments\nend\n", "app/models/post.rb"),
            ("class Story < Article\nend\n", "app/models/story.rb"),
        ],
    );
    assert_eq!(comments_key(&reg, Some("Article"), "build").as_deref(), Some("article_id"));
    assert_eq!(comments_key(&reg, Some("Post"), "build").as_deref(), Some("post_id"));
    assert_eq!(comments_key(&reg, Some("Story"), "find").as_deref(), Some("article_id"));
    assert_eq!(comments_key(&reg, None, "build"), None);
}

/// A parent typed as a model whose chain does not declare the name has
/// no such association, though another model declares it: `@tag.comments`
/// is not built under `article_id`. Only a parent of no known model
/// takes the declaration every declarer agrees on.
#[test]
fn a_model_without_the_declaration_does_not_take_another_models() {
    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
  end
  create_table "tags", force: :cascade do |t|
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
  end
end
"#,
        &[
            ("class Article < ApplicationRecord\n  has_many :comments\nend\n", "app/models/article.rb"),
            ("class Tag < ApplicationRecord\nend\n", "app/models/tag.rb"),
        ],
    );
    assert_eq!(comments_key(&reg, Some("Tag"), "build"), None);
    assert_eq!(comments_key(&reg, Some("Tag"), "find"), None);
    for owner in [None, Some("Current")] {
        assert_eq!(comments_key(&reg, owner, "build").as_deref(), Some("article_id"), "{owner:?}");
    }
}

/// A model declaring `comments` twice keeps the later declaration, for
/// a typed parent and as one declarer among the rest for an untyped one.
#[test]
fn a_redeclared_name_resolves_to_the_later_declaration() {
    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
    t.integer "legacy_article_id"
  end
end
"#,
        &[(
            "class Article < ApplicationRecord\n  has_many :comments\n  has_many :comments, foreign_key: :legacy_article_id\nend\n",
            "app/models/article.rb",
        )],
    );
    for owner in [Some("Article"), None] {
        assert_eq!(comments_key(&reg, owner, "build").as_deref(), Some("legacy_article_id"), "{owner:?}");
    }
}

/// An extension's method answers the call by its name in Rails' place,
/// so that call is declined; the others stay on the declaration.
#[test]
fn an_extension_declines_only_the_call_it_defines() {
    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
  end
end
"#,
        &[(
            "class Article < ApplicationRecord\n  has_many :comments do\n    def find(id)\n      where(id: id).first\n    end\n  end\nend\n",
            "app/models/article.rb",
        )],
    );
    for owner in [Some("Article"), None] {
        assert_eq!(comments_key(&reg, owner, "find"), None, "{owner:?}");
        assert_eq!(comments_key(&reg, owner, "build").as_deref(), Some("article_id"), "{owner:?}");
    }
}

/// `has_many :remarks, class_name: "Feedback"` building from
/// `comment_params`: the typed factory exists only on the model the
/// params resource names (`Comment`), so `Feedback.from_params` would
/// call nothing. The rewrite declines and the call keeps its shape.
#[test]
fn a_nested_build_from_another_resources_params_is_not_expanded() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  add_foreign_key \"comments\", \"articles\"",
            "  create_table \"feedbacks\", force: :cascade do |t|\n    t.integer \"article_id\"\n    t.string \"commenter\"\n    t.text \"body\"\n  end\n\n  add_foreign_key \"comments\", \"articles\"",
        )
        .write("app/models/feedback.rb", "class Feedback < ApplicationRecord\nend\n")
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Feedback\"",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@article.remarks.build("), "{controller}");
    assert!(!controller.contains("Feedback.from_params"), "{controller}");
}

/// `has_many :reviews, class_name: "Comment", foreign_key:
/// :reviewed_article_id`: build presets the declared key, and find
/// answers 404 for a comment the key does not tie to this article, even
/// one whose `article_id` does.
#[test]
fn a_nested_build_and_find_through_a_declared_foreign_key_use_that_key() {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.integer \"article_id\", null: false\n",
            "    t.integer \"article_id\"\n    t.integer \"reviewed_article_id\"\n",
        )
        .edit("app/models/comment.rb", "  belongs_to :article\n", "  belongs_to :article, optional: true\n")
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :reviews, class_name: \"Comment\", foreign_key: :reviewed_article_id",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.reviews.build(comment_params)",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.find(params.expect(:id))",
            "@article.reviews.find(params.expect(:id))",
        )
        .edit(
            "test/controllers/comments_controller_test.rb",
            "    assert_redirected_to article_url(@article)\n  end\n\n  test \"should not create comment with invalid params\" do",
            "    assert_redirected_to article_url(@article)\n    assert_equal @article.id, Comment.last.reviewed_article_id\n    assert_nil Comment.last.article_id\n  end\n\n  test \"should not create comment with invalid params\" do",
        )
        .edit(
            "test/controllers/comments_controller_test.rb",
            "  test \"should destroy comment\" do\n    assert_difference(\"Comment.count\", -1) do\n      delete article_comment_url(@article, @comment)\n    end\n    assert_redirected_to article_url(@article)\n  end",
            "  test \"should destroy comment\" do\n    review = Comment.create!(commenter: \"Rev\", body: \"A review.\", reviewed_article_id: @article.id)\n    assert_difference(\"Comment.count\", -1) do\n      delete article_comment_url(@article, review)\n    end\n    assert_redirected_to article_url(@article)\n  end\n\n  test \"should not destroy a comment reviewing another article\" do\n    assert_no_difference(\"Comment.count\") do\n      delete article_comment_url(@article, @comment)\n    end\n    assert_response :not_found\n  end",
        )
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

const COMMENTABLE: &str = "module Commentable\n  extend ActiveSupport::Concern\n\n  included do\n    has_many :comments, dependent: :destroy\n  end\nend\n";

/// `has_many :comments` declared in a concern's `included do`: the
/// splice puts it on the including model, so a parent typed as that
/// model finds it there and the build expands.
#[test]
fn a_nested_build_through_an_association_a_concern_declares_is_expanded() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .write("app/models/concerns/commentable.rb", COMMENTABLE)
        .edit("app/models/article.rb", "  has_many :comments, dependent: :destroy", "  include Commentable")
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("Comment.from_params"), "{controller}");
    assert!(!controller.contains("@article.comments.build("), "{controller}");
}

/// A concern's `has_many :comments` redeclared after the `include`
/// with another foreign key: the later declaration is the reader the
/// model emits, so build presets that key and find checks it.
#[test]
fn a_nested_build_and_find_through_a_redeclared_association_use_the_later_declaration() {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.integer \"article_id\", null: false\n",
            "    t.integer \"article_id\"\n    t.integer \"legacy_article_id\"\n",
        )
        .edit("app/models/comment.rb", "  belongs_to :article\n", "  belongs_to :article, optional: true\n")
        .write("app/models/concerns/commentable.rb", COMMENTABLE)
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  include Commentable\n  has_many :comments, foreign_key: :legacy_article_id",
        )
        .edit(
            "test/controllers/comments_controller_test.rb",
            "    assert_redirected_to article_url(@article)\n  end\n\n  test \"should not create comment with invalid params\" do",
            "    assert_redirected_to article_url(@article)\n    assert_equal @article.id, Comment.last.legacy_article_id\n    assert_nil Comment.last.article_id\n  end\n\n  test \"should not create comment with invalid params\" do",
        )
        .edit(
            "test/controllers/comments_controller_test.rb",
            "  test \"should destroy comment\" do\n    assert_difference(\"Comment.count\", -1) do\n      delete article_comment_url(@article, @comment)\n    end\n    assert_redirected_to article_url(@article)\n  end",
            "  test \"should destroy comment\" do\n    legacy = Comment.create!(commenter: \"Leg\", body: \"A legacy comment.\", legacy_article_id: @article.id)\n    assert_difference(\"Comment.count\", -1) do\n      delete article_comment_url(@article, legacy)\n    end\n    assert_redirected_to article_url(@article)\n  end\n\n  test \"should not destroy a comment keyed by the earlier declaration\" do\n    assert_no_difference(\"Comment.count\") do\n      delete article_comment_url(@article, @comment)\n    end\n    assert_response :not_found\n  end",
        )
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// The later declaration adds a `where` scope Rails builds with, so
/// the rewrite declines it rather than expand the concern's plain one.
#[test]
fn a_nested_build_and_find_through_a_redeclared_row_changing_association_are_not_expanded() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .write("app/models/concerns/commentable.rb", COMMENTABLE)
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  include Commentable\n  has_many :comments, -> { where(commenter: \"amy\") }",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@article.comments.build("), "{controller}");
    assert!(controller.contains("@article.comments.find("), "{controller}");
    assert!(!controller.contains("Comment.from_params"), "{controller}");
}

/// `@tag.comments` where only Article declares `comments`: Tag has no
/// such association, so neither the rewrite nor the association seed
/// builds or finds a Comment by Article's `article_id`.
#[test]
fn a_nested_build_and_find_on_a_model_without_the_association_do_not_borrow_another_models_key() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  add_foreign_key \"comments\", \"articles\"",
            "  create_table \"tags\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  add_foreign_key \"comments\", \"articles\"",
        )
        .write("app/models/tag.rb", "class Tag < ApplicationRecord\nend\n")
        .edit(
            "app/controllers/comments_controller.rb",
            "      @article = Article.find(params.expect(:article_id))",
            "      @article = Article.find(params.expect(:article_id))\n      @tag = Tag.find(params.expect(:article_id))",
        )
        .edit("app/controllers/comments_controller.rb", "@article.comments.build(comment_params)", "@tag.comments.build(comment_params)")
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.find(params.expect(:id))",
            "@tag.comments.find(params.expect(:id))",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@tag.comments.build("), "{controller}");
    assert!(controller.contains("@tag.comments.find("), "{controller}");
    assert!(
        !controller.lines().any(|l| l.contains("article_id") && l.contains("@tag.id")),
        "{controller}"
    );
}

/// An extension defining `find` is the method `@article.remarks.find`
/// runs: that call reaches it (flattened to `remarks_find`), and
/// `build`, which the extension leaves alone, still expands.
#[test]
fn a_nested_find_through_an_extension_defining_find_reaches_the_extension() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Comment\" do\n    def find(id)\n      Comment.find_by(id: id, commenter: \"amy\")\n    end\n  end",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.find(params.expect(:id))",
            "@article.remarks.find(params.expect(:id))",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@article.remarks_find("), "{controller}");
    assert!(!controller.contains("Comment.find("), "{controller}");
    assert!(controller.contains("Comment.from_params"), "{controller}");
}

/// An extension defining only an unrelated method leaves `build` and
/// `find` as Rails' own, so both still expand through the declaration.
#[test]
fn a_nested_build_and_find_through_an_extension_with_other_methods_use_the_declared_class() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Comment\" do\n    def by(commenter)\n      where(commenter: commenter)\n    end\n  end",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.find(params.expect(:id))",
            "@article.remarks.find(params.expect(:id))",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// CollectionProxy's `new` is `build` under another name, unless an
/// extension defines `new`: then `@article.remarks.new` runs it, and
/// is neither turned into `build` nor expanded.
#[test]
fn a_nested_new_through_an_extension_defining_new_reaches_the_extension() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Comment\" do\n    def new(attributes = {})\n      Comment.new(commenter: \"amy\")\n    end\n  end",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.new(comment_params)",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@article.remarks_new("), "{controller}");
    assert!(!controller.contains("Comment.from_params"), "{controller}");
}
