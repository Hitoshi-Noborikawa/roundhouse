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
//! Part 2 (oracle `nested_attributes3.rb`): an entry with an `id` updates
//! that record of the association (string or Integer id; `RecordNotFound`,
//! with Rails' message, when it is not one), and under `allow_destroy:
//! true` a truthy `_destroy` ("1", "true", "t", "on", `true`; not "0",
//! "false", "f") destroys it: an entry without an `id` so marked builds
//! nothing, and a belongs_to so destroyed leaves the parent's key nil.
//! Without `allow_destroy`, `_destroy` is ignored and the other keys apply.
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

const UPDATE_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
def art
  Article.create!(title: "Nested", body: "A body long enough to validate.",
    comments_attributes: [{ commenter: "c", body: "a" }, { commenter: "c", body: "b" }],
    summary_attributes: { text: "s" }, author_attributes: { name: "n" }, notes_attributes: [{ text: "n1" }])
end
a = art
c = a.comments.first
a.update!(comments_attributes: [{ "id" => c.id.to_s, "body" => "z" }])
check("update by a string id", Comment.find(c.id).body, "z")
a = art
c = a.comments.first
a.update!(comments_attributes: [{ id: c.id, body: "z" }, { commenter: "c", body: "new" }])
check("update and build in one call", Comment.where(article_id: a.id).map(&:body).sort, ["b", "new", "z"])
a = art
c = a.comments.first
check("an invalid update fails the parent", a.update(comments_attributes: [{ id: c.id, body: "" }]), false)
check("its error", a.errors.include?("Comments body can't be blank"), true)
check("nothing written", Comment.find(c.id).body, "a")
a = art
missing = begin
  a.update(comments_attributes: [{ id: 0, body: "q" }])
  "no error"
rescue ActiveRecord::RecordNotFound => e
  e.message
end
check("an unknown id", missing, "Couldn't find Comment with ID=0 for Article with ID=#{a.id}")
["1", "true", "t", "TRUE", "on", true].each do |v|
  a = art
  c = a.comments.first
  a.update!(comments_attributes: [{ id: c.id, _destroy: v }])
  check("_destroy #{v.inspect} destroys", Comment.where(id: c.id).count, 0)
end
["0", "false", "f"].each do |v|
  a = art
  c = a.comments.first
  a.update!(comments_attributes: [{ id: c.id, _destroy: v }])
  check("_destroy #{v.inspect} keeps", Comment.where(id: c.id).count, 1)
end
a = art
a.update!(comments_attributes: [{ commenter: "c", body: "x", _destroy: "1" }])
check("a marked entry without an id builds nothing", Comment.where(article_id: a.id).map(&:body).sort, ["a", "b"])
a = art
c = a.comments.first
a.update!(comments_attributes: [{ id: c.id, body: "zz", _destroy: "1" }])
check("marked with attributes: destroyed", Comment.where(id: c.id).count, 0)
a = art
n = Note.where(article_id: a.id).first
a.update!(notes_attributes: [{ id: n.id, text: "t2", _destroy: "1" }])
check("no allow_destroy: kept and updated", Note.find(n.id).text, "t2")
a = art
s = Summary.where(article_id: a.id).first
a.update!(summary_attributes: { id: s.id, text: "t2" })
check("has_one update", Summary.find(s.id).text, "t2")
a = art
s = Summary.where(article_id: a.id).first
a.update!(summary_attributes: { id: s.id, _destroy: "1" })
check("has_one destroy", Summary.where(id: s.id).count, 0)
a = art
au = Author.find(a.author_id)
a.update!(author_attributes: { id: au.id, name: "m" })
check("belongs_to update", Author.find(au.id).name, "m")
a = art
au = Author.find(a.author_id)
a.update!(author_attributes: { id: au.id, _destroy: "1" })
check("belongs_to destroy", [Author.where(id: au.id).count, Article.find(a.id).author_id], [0, nil])
a = art
wrong = begin
  a.update(author_attributes: { id: 0, name: "m" })
  "no error"
rescue ActiveRecord::RecordNotFound => e
  e.message
end
check("belongs_to wrong id", wrong, "Couldn't find Author with ID=0 for Article with ID=#{a.id}")
puts "nested update passed"
"##;

#[test]
fn nested_attributes_update_and_destroy_on_ruby() {
    let run = app().run_ruby(UPDATE_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested update passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn nested_attributes_update_and_destroy_on_spinel() {
    let run = app().run_spinel_with_db(UPDATE_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested update passed"), "{}", run.stdout);
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
