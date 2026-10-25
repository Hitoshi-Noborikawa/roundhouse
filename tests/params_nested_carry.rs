//! Non-scalar permitted keys reach the model (#1338). activerecord /
//! actionpack 8.1.4 (oracles `nested_permit.rb`, `nested_permit2.rb`):
//! `permit(…, comments_attributes: [:commenter, :body])` and
//! `expect(article: […, comments_attributes: [[:commenter, :body]]])`
//! keep the nested value, an Array of hashes or a hash of hashes
//! (`{"0" => {…}}`), each hash cut to the listed keys, and
//! `Article.new(permitted)` hands it to `comments_attributes=`.
//!
//! `permit(…, labels: [])` keeps an Array of scalars, and drops one
//! holding a hash or an array, a scalar, or a hash (`nested_permit2.rb`);
//! the ruby family carries it as an `Array[String]`.
//!
//! On the ruby family the key was dropped from the synthesized params
//! record with a "non-scalar" warning, so the writer never ran. Strict
//! targets keep that warning.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const WRITER: &str = "  has_many :comments, dependent: :destroy\n\n  def comments_attributes=(list)\n    entries = list.is_a?(Hash) ? list.values : list\n    @pending_comments = []\n    entries.each { |entry| @pending_comments << entry }\n  end\n\n  after_save :save_pending_comments\n\n  def save_pending_comments\n    pending = @pending_comments\n    return if pending.nil?\n    pending.each { |entry| Comment.create!(article_id: id, commenter: entry.keys.sort.join(\"+\"), body: entry[\"body\"]) }\n    @pending_comments = []\n  end\n";

const TEST: &str = r#"
  test "nested comments as an array" do
    post articles_url, params: { article: { title: "Nested", body: "A sufficiently long body for validation.",
      comments_attributes: [{ commenter: "c1", body: "one", evil: "x" }, { commenter: "c2", body: "two" }] } }
    comments = Comment.where(article_id: Article.last.id).order(:id)
    assert_equal ["one", "two"], comments.map(&:body)
    assert_equal ["body+commenter", "body+commenter"], comments.map(&:commenter)
  end

  test "nested comments as a hash of hashes" do
    post articles_url, params: { article: { title: "Nested", body: "A sufficiently long body for validation.",
      comments_attributes: { "0" => { commenter: "c1", body: "one" }, "1" => { commenter: "c2", body: "two", evil: "x" } } } }
    comments = Comment.where(article_id: Article.last.id).order(:id)
    assert_equal ["one", "two"], comments.map(&:body)
    assert_equal ["body+commenter", "body+commenter"], comments.map(&:commenter)
  end

  test "an update carries them too" do
    article = Article.create!(title: "Nested", body: "A sufficiently long body for validation.")
    patch article_url(article), params: { article: { comments_attributes: [{ commenter: "c", body: "later" }] } }
    assert_equal ["later"], Comment.where(article_id: article.id).map(&:body)
  end

"#;

fn app(helper: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", WRITER)
        .edit("app/controllers/articles_controller.rb", "      params.expect(article: [ :title, :body ])\n", helper)
        .edit("test/controllers/articles_controller_test.rb", "  test \"should get new\" do\n", &format!("{TEST}  test \"should get new\" do\n"))
}

const EXPECT: &str = "      params.expect(article: [ :title, :body, comments_attributes: [ [ :commenter, :body ] ] ])\n";
const PERMIT: &str = "      params.require(:article).permit(:title, :body, comments_attributes: [ :commenter, :body ])\n";

#[test]
fn expect_list_reaches_the_writer_on_ruby() {
    app(EXPECT).run_test("test/controllers/articles_controller_test.rb").assert_passes();
}

#[test]
fn permit_list_reaches_the_writer_on_ruby() {
    app(PERMIT).run_test("test/controllers/articles_controller_test.rb").assert_passes();
}

const SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
list = [{ "commenter" => "c1", "body" => "one", "evil" => "x" }, { "commenter" => "c2", "body" => "two" }]
raw = { "article" => { "title" => "Nested", "body" => "A sufficiently long body for validation.", "comments_attributes" => list } }
a = Article.from_params(ArticleParams.from_raw(raw))
check("saved", a.save, true)
comments = Comment.where(article_id: a.id).order(:id)
check("an array", comments.map(&:body), ["one", "two"])
check("filtered", comments.map(&:commenter), ["body+commenter", "body+commenter"])
numbered = { "0" => { "commenter" => "c1", "body" => "three" }, "x" => { "commenter" => "c", "body" => "skipped" } }
raw = { "article" => { "title" => "Nested", "body" => "A sufficiently long body for validation.", "comments_attributes" => numbered } }
b = Article.from_params(ArticleParams.from_raw(raw))
check("saved", b.save, true)
check("a hash of hashes", Comment.where(article_id: b.id).map(&:body), ["three"])
puts "nested carry passed"
"##;

#[test]
fn from_raw_carries_the_list_on_ruby() {
    let run = app(EXPECT).run_ruby(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested carry passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn from_raw_carries_the_list_on_spinel() {
    let run = app(EXPECT).run_spinel_with_db(SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("nested carry passed"), "{}", run.stdout);
}

const LABELS_WRITER: &str = "  has_many :comments, dependent: :destroy\n\n  def labels=(list)\n    self.body = body.to_s + \" [\" + list.join(\",\") + \"]\"\n  end\n";
const LABELS_PERMIT: &str = "      params.require(:article).permit(:title, :body, labels: [])\n";

const LABELS_TEST: &str = r#"
  test "a scalar list" do
    post articles_url, params: { article: { title: "Labels", body: "A sufficiently long body for validation.", labels: ["a", "b"] } }
    assert_equal "A sufficiently long body for validation. [a,b]", Article.last.body
  end

  test "a list holding a hash is dropped" do
    post articles_url, params: { article: { title: "Labels", body: "A sufficiently long body for validation.", labels: ["a", { x: "1" }] } }
    assert_equal "A sufficiently long body for validation.", Article.last.body
  end

"#;

fn labels_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", LABELS_WRITER)
        .edit("app/controllers/articles_controller.rb", "      params.expect(article: [ :title, :body ])\n", LABELS_PERMIT)
        .edit("test/controllers/articles_controller_test.rb", "  test \"should get new\" do\n", &format!("{LABELS_TEST}  test \"should get new\" do\n"))
}

#[test]
fn a_scalar_list_reaches_the_writer_on_ruby() {
    labels_app().run_test("test/controllers/articles_controller_test.rb").assert_passes();
}

const LABELS_SCRIPT: &str = r##"
def check(label, got, want)
  raise "#{label}: #{got.inspect} (want #{want.inspect})" unless got == want
end
raw = { "article" => { "title" => "Labels", "body" => "A sufficiently long body for validation.", "labels" => ["a", "b"] } }
a = Article.from_params(ArticleParams.from_raw(raw))
check("a scalar list", a.body, "A sufficiently long body for validation. [a,b]")
raw = { "article" => { "title" => "Labels", "body" => "A sufficiently long body for validation.", "labels" => "a" } }
b = Article.from_params(ArticleParams.from_raw(raw))
check("a scalar is dropped", b.body, "A sufficiently long body for validation.")
puts "labels passed"
"##;

#[test]
fn from_raw_carries_a_scalar_list_on_ruby() {
    let run = labels_app().run_ruby(LABELS_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("labels passed"), "{}", run.stdout);
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn from_raw_carries_a_scalar_list_on_spinel() {
    let run = labels_app().run_spinel_with_db(LABELS_SCRIPT);
    run.assert_passes();
    assert!(run.stdout.contains("labels passed"), "{}", run.stdout);
}

/// A strict target's params record still holds Strings only: the key
/// stays dropped there, with its warning; the ruby family carries it.
#[test]
fn a_strict_target_keeps_the_warning() {
    let root = std::env::temp_dir().join(format!("roundhouse-params-nested-carry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("app");
    std::fs::create_dir_all(&root).unwrap();
    assert!(std::process::Command::new("cp").args(["-a"]).arg(roundhouse::fixtures::real_blog()).arg(&source).status().unwrap().success());
    let edit = |path: &str, find: &str, replace: &str| {
        let file = source.join(path);
        let text = std::fs::read_to_string(&file).unwrap().replacen(find, replace, 1);
        std::fs::write(file, text).unwrap();
    };
    edit("app/models/article.rb", "  has_many :comments, dependent: :destroy\n", WRITER);
    edit("app/controllers/articles_controller.rb", "      params.expect(article: [ :title, :body ])\n", EXPECT);
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
    let dropped = "permitted key `comments_attributes` on `article`";
    assert!(rust.contains(dropped), "{rust}");
    assert!(!ruby.contains(dropped), "{ruby}");
}
