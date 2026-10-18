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

/// An owner expression: `"@story"` is an ivar of no analyzed type,
/// `"@parent: Article"` one analyzed as Article, and `"@owner: Tag |
/// Article"` one analyzed as that union.
fn owner(written: &str) -> roundhouse::expr::Expr {
    use roundhouse::expr::{Expr, ExprNode};
    use roundhouse::ident::{ClassId, Symbol};
    use roundhouse::ty::Ty;

    let (name, ty) = match written.split_once(": ") {
        Some((name, ty)) => (name, Some(ty)),
        None => (written, None),
    };
    let mut owner = Expr::new(
        roundhouse::span::Span::synthetic(),
        ExprNode::Ivar { name: Symbol::from(name.trim_start_matches('@')) },
    );
    owner.ty = ty.map(|ty| {
        let mut arms: Vec<Ty> =
            ty.split(" | ").map(|c| Ty::Class { id: ClassId(Symbol::from(c)), args: vec![].into() }).collect();
        if arms.len() == 1 { arms.remove(0) } else { Ty::Union { variants: arms.into() } }
    });
    owner
}

/// What `<owner>.<assoc>.<call>` builds or finds through.
fn resolve(
    reg: &roundhouse::lower::scope_chain::AssocRegistry,
    written: &str,
    assoc: &str,
    call: &str,
) -> roundhouse::lower::scope_chain::AssocBuildTarget {
    use roundhouse::ident::Symbol;
    reg.build_target(&owner(written), &Symbol::from(assoc), &Symbol::from(call))
}

/// What `<owner>.comments.<call>` builds or finds through.
fn comments(
    reg: &roundhouse::lower::scope_chain::AssocRegistry,
    written: &str,
    call: &str,
) -> roundhouse::lower::scope_chain::AssocBuildTarget {
    resolve(reg, written, "comments", call)
}

/// A `Comment` built or found under `foreign_key`.
fn comment_by(foreign_key: &str) -> roundhouse::lower::scope_chain::AssocBuildTarget {
    use roundhouse::ident::{ClassId, Symbol};
    roundhouse::lower::scope_chain::AssocBuildTarget::Declared {
        target: ClassId(Symbol::from("Comment")),
        foreign_key: Symbol::from(foreign_key),
    }
}

use roundhouse::lower::scope_chain::AssocBuildTarget::{Declined, Undeclared};

/// The parent's class picks between declarations, whatever its name. A
/// subclass finds the declaration it inherits.
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
    assert_eq!(comments(&reg, "@post: Article", "build"), comment_by("article_id"));
    assert_eq!(comments(&reg, "@article: Post", "build"), comment_by("post_id"));
    assert_eq!(comments(&reg, "@parent: Story", "find"), comment_by("article_id"));
    assert_eq!(comments(&reg, "@parent: Story | Article", "build"), comment_by("article_id"));
    assert_eq!(comments(&reg, "@parent: Article | Post", "build"), Declined);
    assert_eq!(comments(&reg, "@parent", "build"), Declined);
}

/// A parent of no known model resolves by its name when the name is a
/// model that (itself or through a superclass) declares the association,
/// as the association seed resolves it, so a typed-params build and a
/// kwargs build on the same parent agree. A name that is no model, or a
/// model without the association, leaves only the declarers' agreement.
#[test]
fn an_untyped_parent_resolves_by_its_name_like_the_seed() {
    use roundhouse::ident::Symbol;
    use roundhouse::lower::scope_chain::assoc_read_target;

    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
    t.string "type"
  end
  create_table "users", force: :cascade do |t|
  end
  create_table "tags", force: :cascade do |t|
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
    t.integer "user_id"
  end
end
"#,
        &[
            ("class Article < ApplicationRecord\n  has_many :comments\nend\n", "app/models/article.rb"),
            ("class Story < Article\nend\n", "app/models/story.rb"),
            ("class User < ApplicationRecord\n  has_many :comments\nend\n", "app/models/user.rb"),
            ("class Tag < ApplicationRecord\nend\n", "app/models/tag.rb"),
        ],
    );
    let seed = |written: &str| {
        assoc_read_target(&owner(written), &Symbol::from("comments"), &reg)
            .map(|(model, _, fk)| (model.map(|m| m.0.as_str().to_string()), fk.as_str().to_string()))
    };
    assert_eq!(comments(&reg, "@user", "build"), comment_by("user_id"));
    assert_eq!(seed("@user"), Some((Some("User".to_string()), "user_id".to_string())));
    assert_eq!(comments(&reg, "@story", "build"), comment_by("article_id"));
    assert_eq!(seed("@story"), Some((Some("Article".to_string()), "article_id".to_string())));
    for written in ["@parent", "@tag"] {
        assert_eq!(comments(&reg, written, "build"), Declined, "{written}");
        assert_eq!(seed(written), None, "{written}");
    }
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
    assert_eq!(comments(&reg, "@tag: Tag", "build"), Declined);
    assert_eq!(comments(&reg, "@tag: Tag", "find"), Declined);
    assert_eq!(comments(&reg, "@owner: Tag | Article", "build"), Declined);
    for written in ["@parent", "@parent: Current", "@tag"] {
        assert_eq!(comments(&reg, written, "build"), comment_by("article_id"), "{written}");
    }
}

/// A name no model declares (an association from a `with_options` block
/// or a custom macro, which the registry does not see) is `Undeclared`,
/// for the rewrite's name-derived form, even on a parent typed as a
/// model. One another model declares stays declined for a model that
/// does not (above).
#[test]
fn a_name_no_model_declares_is_undeclared_on_any_parent() {
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
        &[("class Article < ApplicationRecord\nend\n", "app/models/article.rb")],
    );
    for written in ["@article: Article", "@article", "@owner: Article | Article"] {
        for call in ["build", "find", "find_by"] {
            assert_eq!(comments(&reg, written, call), Undeclared, "{written}.comments.{call}");
        }
    }
}

/// A union arm whose chain declares no `comments` has no such reader,
/// so a parent named after the arm that declares it is that arm, for
/// the rewrite and the seed alike (campfire's `@room` typed `Membership
/// | Room`). The name does not choose between arms declaring it
/// differently, nor name a model outside the union.
#[test]
fn a_union_typed_parent_named_after_its_declaring_arm_reads_that_arm() {
    use roundhouse::ident::Symbol;
    use roundhouse::lower::scope_chain::assoc_read_target;

    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
  end
  create_table "posts", force: :cascade do |t|
  end
  create_table "tags", force: :cascade do |t|
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
            ("class Tag < ApplicationRecord\nend\n", "app/models/tag.rb"),
        ],
    );
    let seed = |written: &str| {
        assoc_read_target(&owner(written), &Symbol::from("comments"), &reg)
            .map(|(model, _, fk)| (model.map(|m| m.0.as_str().to_string()), fk.as_str().to_string()))
    };
    assert_eq!(comments(&reg, "@article: Tag | Article", "build"), comment_by("article_id"));
    assert_eq!(comments(&reg, "@article: Tag | Article", "find"), comment_by("article_id"));
    assert_eq!(seed("@article: Tag | Article"), Some((Some("Article".to_string()), "article_id".to_string())));
    for written in ["@tag: Tag | Article", "@post: Tag | Article", "@article: Article | Post"] {
        assert_eq!(comments(&reg, written, "build"), Declined, "{written}");
        assert_eq!(seed(written), None, "{written}");
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
    for written in ["@parent: Article", "@parent"] {
        assert_eq!(comments(&reg, written, "build"), comment_by("legacy_article_id"), "{written}");
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
    for written in ["@parent: Article", "@parent"] {
        assert_eq!(comments(&reg, written, "find"), Declined, "{written}");
        assert_eq!(comments(&reg, written, "build"), comment_by("article_id"), "{written}");
    }
}

/// A scope is judged per call. Ordering decides which match `find_by`
/// takes, so it declines `find_by` alone; what `build` makes and the row
/// `find(id)` names do not depend on it. Eager loading, `references` and
/// `readonly` change none of them; a `where` changes all of them.
#[test]
fn a_scope_declines_only_the_calls_it_changes() {
    let reg = registry(
        r#"
ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "articles", force: :cascade do |t|
  end
  create_table "comments", force: :cascade do |t|
    t.integer "article_id"
    t.string "commenter"
    t.datetime "created_at"
  end
end
"#,
        &[(
            "class Article < ApplicationRecord\n  has_many :ordered, -> { order(created_at: :desc) }, class_name: \"Comment\"\n  has_many :reordered, -> { includes(:article).reorder(:id) }, class_name: \"Comment\"\n  has_many :locked, -> { readonly }, class_name: \"Comment\"\n  has_many :referenced, -> { includes(:article).references(:article) }, class_name: \"Comment\"\n  has_many :filtered, -> { where(commenter: \"amy\") }, class_name: \"Comment\"\nend\n",
            "app/models/article.rb",
        )],
    );
    let builds_and_finds = ["build", "new", "create", "create!", "find"];
    for assoc in ["ordered", "reordered"] {
        for call in builds_and_finds {
            assert_eq!(resolve(&reg, "@article: Article", assoc, call), comment_by("article_id"), "{assoc}.{call}");
        }
        assert_eq!(resolve(&reg, "@article: Article", assoc, "find_by"), Declined, "{assoc}.find_by");
    }
    for assoc in ["locked", "referenced"] {
        for call in builds_and_finds.iter().chain(&["find_by"]) {
            assert_eq!(resolve(&reg, "@article: Article", assoc, call), comment_by("article_id"), "{assoc}.{call}");
        }
    }
    for call in builds_and_finds.iter().chain(&["find_by"]) {
        assert_eq!(resolve(&reg, "@article: Article", "filtered", call), Declined, "filtered.{call}");
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

/// `@owner` holding a Tag or an Article: only the Article has
/// `comments`, so neither the rewrite nor the association seed builds
/// or finds a Comment under the Tag's id as its `article_id`. The
/// kwargs build and the `find` reach the seed, which the rewrite leaves
/// them to.
#[test]
fn a_nested_build_on_a_union_of_models_does_not_borrow_one_arms_key() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  add_foreign_key \"comments\", \"articles\"",
            "  create_table \"tags\", force: :cascade do |t|\n    t.string \"name\"\n  end\n\n  add_foreign_key \"comments\", \"articles\"",
        )
        .write("app/models/tag.rb", "class Tag < ApplicationRecord\nend\n")
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.build(comment_params)",
            "    @owner = params[:tag] ? Tag.find(1) : @article\n    @comment = @owner.comments.build(comment_params)\n    @draft = @owner.comments.build(commenter: \"amy\")",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.find(params.expect(:id))",
            "    @owner = params[:tag] ? Tag.find(1) : @article\n    @comment = @owner.comments.find(params.expect(:id))",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@owner.comments.build(self.comment_params)"), "{controller}");
    assert!(controller.contains("@owner.comments.build(commenter: \"amy\")"), "{controller}");
    assert!(controller.contains("@owner.comments.find("), "{controller}");
    assert!(
        !controller.lines().any(|l| l.contains("article_id") && l.contains("@owner.id") || l.contains("article: @owner")),
        "{controller}"
    );
}

/// Post's `comments` extension defines `count`; Article's is Rails'
/// own. `@article.comments.count` counts Article's comments rather than
/// calling a `comments_count` Article does not have.
#[test]
fn a_typed_owner_ignores_another_models_extension() {
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
        .write(
            "app/models/post.rb",
            "class Post < ApplicationRecord\n  has_many :comments do\n    def count\n      0\n    end\n  end\nend\n",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.build(comment_params)\n",
            "    @comment = @article.comments.build(comment_params)\n    @comment_count = @article.comments.count\n",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
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

/// The extension defining `new` comes with the later declaration of
/// `remarks`, the one whose reader runs, so `new` still reaches it.
#[test]
fn a_nested_new_through_a_redeclared_extension_defining_new_reaches_the_extension() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Comment\"\n  has_many :remarks, class_name: \"Comment\" do\n    def new(attributes = {})\n      Comment.new(commenter: \"amy\")\n    end\n  end",
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

/// `has_many :remarks, -> { order(created_at: :desc) }`: Rails' `find_by`
/// takes the first match in that order, which `Comment.find_by(…,
/// article_id:)` would not, so the rewrite declines `find_by` while
/// `build` and `find` still expand.
#[test]
fn a_nested_find_by_through_an_ordering_scope_is_not_expanded() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, -> { order(created_at: :desc) }, class_name: \"Comment\"",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.build(comment_params)",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.find(params.expect(:id))",
            "    @comment = @article.remarks.find(params.expect(:id))\n    @latest = @article.remarks.find_by(commenter: @comment.commenter)",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("@article.remarks.find_by("), "{controller}");
    assert!(!controller.contains("Comment.find_by("), "{controller}");
    assert!(controller.contains("Comment.from_params"), "{controller}");
    assert!(controller.contains("Comment.find("), "{controller}");
}

/// `@story` of no analyzed type, while Story and Article both declare
/// `comments`: the name says Story, as it does for the association seed,
/// so the typed-params build expands under `story_id` rather than being
/// left on the reader.
#[test]
fn a_nested_build_on_an_untyped_parent_named_after_a_declarer_uses_its_key() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.integer \"article_id\", null: false\n",
            "    t.integer \"article_id\"\n    t.integer \"story_id\"\n",
        )
        .edit(
            "db/schema.rb",
            "  add_foreign_key \"comments\", \"articles\"",
            "  create_table \"stories\", force: :cascade do |t|\n    t.string \"title\"\n  end\n\n  add_foreign_key \"comments\", \"articles\"",
        )
        .write("app/models/story.rb", "class Story < ApplicationRecord\n  has_many :comments\nend\n")
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@story.comments.build(comment_params)",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("Comment.from_params"), "{controller}");
    assert!(controller.lines().any(|l| l.contains("story_id") && l.contains("@story.id")), "{controller}");
    assert!(!controller.contains("@story.comments.build("), "{controller}");
}

/// An extension defining `build` does not change `new`: Rails binds the
/// proxy's `new` to its own `build` where the alias is defined, so
/// `@article.remarks.new(comment_params)` builds a Comment of the
/// declaration with those params, not what the extension's `build`
/// makes.
#[test]
fn a_nested_new_through_an_extension_defining_build_builds_through_the_declaration() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :remarks, class_name: \"Comment\" do\n    def build(attributes = {})\n      Comment.new(commenter: \"amy\")\n    end\n  end",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.remarks.new(comment_params)",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// `Article < Publication`, an abstract base declaring `comments`:
/// `@article.comments` is the association Article inherits, so its `new`
/// is `build` there too, and the seed presets the declared key.
#[test]
fn a_new_through_an_inherited_association_is_build() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .write(
            "app/models/publication.rb",
            "class Publication < ApplicationRecord\n  self.abstract_class = true\n  has_many :comments, foreign_key: :article_id, dependent: :destroy\nend\n",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n",
            "class Article < Publication\n",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.build(comment_params)\n",
            "    @comment = @article.comments.build(comment_params)\n    draft = @article.comments.new(commenter: \"amy\")\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(
        controller.contains("draft = Comment.new(commenter: \"amy\", article_id: @article.id)"),
        "{controller}"
    );
}

/// `create` and `find_by` through `class_name:` and `foreign_key:`:
/// create presets the declared key, and `find_by` finds nothing for a
/// comment the key does not tie to this article, even one whose
/// `article_id` does.
#[test]
fn a_nested_create_and_find_by_through_a_declared_foreign_key_use_that_key() {
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
            "    @comment = @article.comments.build(comment_params)\n    if @comment.save",
            "    @comment = @article.reviews.create(comment_params)\n    if @comment.persisted?",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.find(params.expect(:id))\n    @comment.destroy\n    redirect_to @article, notice: \"Comment was successfully deleted.\"",
            "    @comment = @article.reviews.find_by(id: params.expect(:id))\n    if @comment\n      @comment.destroy\n      redirect_to @article, notice: \"Comment was successfully deleted.\"\n    else\n      head :not_found\n    end",
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

/// `create!` through the same declaration: the typed factory of the
/// declared class, saved with the declared key preset.
#[test]
fn a_nested_create_bang_through_a_declared_foreign_key_uses_that_key() {
    let (emitted, _errors) = emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "    t.integer \"article_id\", null: false\n",
            "    t.integer \"article_id\"\n    t.integer \"reviewed_article_id\"\n",
        )
        .edit(
            "app/models/article.rb",
            "  has_many :comments, dependent: :destroy",
            "  has_many :comments, dependent: :destroy\n  has_many :reviews, class_name: \"Comment\", foreign_key: :reviewed_article_id",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "@article.comments.build(comment_params)",
            "@article.reviews.create!(comment_params)",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    let controller = std::fs::read_to_string(emitted.join("app/controllers/comments_controller.rb"))
        .expect("emitted controller");
    assert!(controller.contains("Comment.from_params"), "{controller}");
    assert!(controller.lines().any(|l| l.contains("reviewed_article_id") && l.contains("@article.id")), "{controller}");
    assert!(!controller.contains("@article.reviews.create!("), "{controller}");
}

/// Campfire's `@membership, @room = RecordCache.fetch(…) { [membership,
/// membership.room] }` types `@room` as `Membership | Room`, as this
/// types `@article` `Comment | Article`. Only the Article arm has
/// `comments`, and the ivar is named after it, so a concern's class
/// method called through the association (campfire's
/// `@room.messages.create_with_attachment!(message_params)`) runs
/// against the article's comments: it takes the relation, the foreign
/// key is preset, and the params arrive as an attribute hash. The
/// `find` in `destroy` is checked against the same key.
#[test]
fn a_class_method_through_an_association_on_a_union_typed_owner_named_after_its_arm_takes_the_scope() {
    let (find, replace) = CREATED_UNDER_THE_ARTICLE;
    emit_and_run::real_blog()
        .write(
            "app/models/comment/noting.rb",
            "module Comment::Noting\n  extend ActiveSupport::Concern\n\n  module ClassMethods\n    def create_noted!(attributes)\n      create!(attributes)\n    end\n  end\nend\n",
        )
        .edit("app/models/comment.rb", "class Comment < ApplicationRecord\n", "class Comment < ApplicationRecord\n  include Noting\n\n")
        .edit(
            "app/controllers/comments_controller.rb",
            "      @article = Article.find(params.expect(:article_id))",
            "      @first_comment, @article = [ Comment.first, Article.find(params.expect(:article_id)) ]",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment = @article.comments.build(comment_params)\n    if @comment.save\n      redirect_to @article, notice: \"Comment was successfully created.\"\n    else\n      redirect_to @article, alert: \"Could not create comment.\"\n    end\n  end",
            "    @comment = @article.comments.create_noted!(comment_params)\n    redirect_to @article, notice: \"Comment was successfully created.\"\n  rescue ActiveRecord::RecordInvalid\n    redirect_to @article, alert: \"Could not create comment.\"\n  end",
        )
        .edit("test/controllers/comments_controller_test.rb", find, replace)
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}

/// The same union-typed `@article` reading `comments` through a scope
/// and through a class method that queries: each reads the article's
/// comments alone. Destroying article one's only comment leaves it
/// none while article two keeps its own.
#[test]
fn a_scope_and_a_query_class_method_through_an_association_on_a_union_typed_owner_read_its_rows() {
    emit_and_run::real_blog()
        .edit(
            "app/models/comment.rb",
            "class Comment < ApplicationRecord\n",
            "class Comment < ApplicationRecord\n  scope :recent, -> { order(created_at: :desc) }\n\n  def self.tally\n    count\n  end\n\n",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "      @article = Article.find(params.expect(:article_id))",
            "      @first_comment, @article = [ Comment.first, Article.find(params.expect(:article_id)) ]",
        )
        .edit(
            "app/controllers/comments_controller.rb",
            "    @comment.destroy\n    redirect_to @article, notice: \"Comment was successfully deleted.\"",
            "    @comment.destroy\n    if @article.comments.tally == 0 && @article.comments.recent.first.nil?\n      redirect_to @article, notice: \"Comment was successfully deleted.\"\n    else\n      head :conflict\n    end",
        )
        .run_test("test/controllers/comments_controller_test.rb")
        .assert_passes();
}
