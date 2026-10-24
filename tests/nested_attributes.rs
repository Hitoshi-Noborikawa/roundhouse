//! `accepts_nested_attributes_for` (#1335, part 1: the writer and building
//! new records). activerecord 8.1.4 (oracles `nested_attributes.rb`,
//! `nested_attributes2.rb`): `<assoc>_attributes=` builds a record per
//! entry (an Array of hashes, or a hash of hashes) for a has_many, one for
//! a has_one or belongs_to, including several associations named in one
//! call and one declared in a concern's `included do`; the records are
//! saved with the parent, a belongs_to one first; an invalid one fails the
//! parent's save with nothing written, its error under the association
//! ("Comments body can't be blank").
//!
//! The declaration was reported and dropped, and no writer existed.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const TABLES: &str = "  create_table \"comments\", force: :cascade do |t|";
const NEW_TABLES: &str = "  create_table \"authors\", force: :cascade do |t|\n    t.string \"name\"\n    t.string \"role\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"summaries\", force: :cascade do |t|\n    t.integer \"article_id\"\n    t.string \"text\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"notes\", force: :cascade do |t|\n    t.integer \"article_id\"\n    t.string \"text\"\n    t.datetime \"created_at\", null: false\n    t.datetime \"updated_at\", null: false\n  end\n\n  create_table \"comments\", force: :cascade do |t|";

const ARTICLE: &str = "class Article < ApplicationRecord\n  include Annotated\n  has_many :comments, dependent: :destroy\n  has_one :summary\n  belongs_to :author, optional: true\n  accepts_nested_attributes_for :comments, :summary, :author, allow_destroy: true\n";

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
base = { title: "Nested", body: "A body long enough to validate." }
a = Article.new(**base,
  comments_attributes: [{ commenter: "c1", body: "one" }, { commenter: "c2", body: "two" }],
  summary_attributes: { text: "short" },
  author_attributes: { name: "Ann" },
  notes_attributes: [{ text: "n" }])
check("saved", a.save, true)
found = Article.find(a.id)
check("has_many built", found.comments.map(&:body).sort, ["one", "two"])
check("has_one built", Summary.where(article_id: a.id).first.text, "short")
check("belongs_to built first", Author.find(found.author_id).name, "Ann")
check("concern's included do", Note.where(article_id: a.id).count, 1)
h = Article.create!(**base, comments_attributes: { "0" => { "commenter" => "s", "body" => "string keys" } })
check("hash of hashes, string keys", Article.find(h.id).comments.map(&:body), ["string keys"])
extra = { name: "Bo" }
extra[:role] = "editor"
e = Article.create!(**base, author_attributes: extra)
check("a key set before assignment", Author.find(Article.find(e.id).author_id).role, "editor")
before = [Article.count, Comment.count]
bad = Article.new(**base, comments_attributes: [{ commenter: "c", body: "" }])
check("an invalid child fails the parent", bad.save, false)
check("its error under the association", bad.errors.include?("Comments body can't be blank"), true)
check("nothing written", [Article.count, Comment.count], before)
bad_author = Article.new(**base, author_attributes: { name: "" })
check("an invalid belongs_to fails the parent", bad_author.save, false)
u = Article.create!(**base)
u.update!(comments_attributes: [{ commenter: "late", body: "added" }])
check("update builds", Article.find(u.id).comments.map(&:body), ["added"])
raised = begin
  u.update(comments_attributes: [{ id: u.comments.first.id, body: "z" }])
  "no error"
rescue NotImplementedError
  "NotImplementedError"
end
check("an entry with an id is part 2", raised, "NotImplementedError")
puts "nested attributes passed"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("db/schema.rb", TABLES, NEW_TABLES)
        .edit("db/schema.rb", "    t.string \"title\"\n", "    t.string \"title\"\n    t.integer \"author_id\"\n")
        .edit("app/models/article.rb", "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n", ARTICLE)
        .write("app/models/author.rb", "class Author < ApplicationRecord\n  validates :name, presence: true\nend\n")
        .write("app/models/summary.rb", "class Summary < ApplicationRecord\n  belongs_to :article\nend\n")
        .write("app/models/note.rb", "class Note < ApplicationRecord\n  belongs_to :article\nend\n")
        .write(
            "app/models/concerns/annotated.rb",
            "module Annotated\n  extend ActiveSupport::Concern\n\n  included do\n    has_many :notes\n    accepts_nested_attributes_for :notes\n  end\nend\n",
        )
}

#[test]
fn nested_attributes_build_and_save_with_the_parent_on_ruby() {
    let run = app().run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested attributes passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn nested_attributes_build_and_save_with_the_parent_on_spinel() {
    let run = app().run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested attributes passed"), "{}", run.stdout);
}


/// A strict target's lowering of the generated source is not verified,
/// so there the declaration stays the reported gap it was.
#[test]
fn a_strict_target_keeps_the_declaration_reported() {
    let root = std::env::temp_dir().join(format!("roundhouse-nested-attributes-strict-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("app");
    std::fs::create_dir_all(&root).unwrap();
    assert!(std::process::Command::new("cp").args(["-a"]).arg(roundhouse::fixtures::real_blog()).arg(&source).status().unwrap().success());
    let model = std::fs::read_to_string(source.join("app/models/article.rb")).unwrap().replacen(
        "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n",
        "class Article < ApplicationRecord\n  has_many :comments, dependent: :destroy\n  accepts_nested_attributes_for :comments\n",
        1,
    );
    std::fs::write(source.join("app/models/article.rb"), model).unwrap();
    let run = |target: &str| {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_roundhouse"))
            .args(["--target", target, "--allow-unsupported", "-o"])
            .arg(root.join(format!("out-{target}")))
            .arg(&source)
            .output()
            .expect("run roundhouse");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let rust = run("rust");
    let ruby = run("ruby");
    std::fs::remove_dir_all(&root).ok();
    assert!(rust.contains("accepts_nested_attributes_for not supported"), "{rust}");
    assert!(!ruby.contains("accepts_nested_attributes_for not supported"), "{ruby}");
}
