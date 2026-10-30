//! `record.slice(:id, :title)` answers Rails' `ActiveRecord::Core#slice`: the named attributes, String-keyed. Campfire's typing channel broadcasts `current_user.slice(:id, :name)`, and without the method every typing notification raised.

use super::emit_and_run;

const SUMMARY: &str = "  def summary = slice(:id, :title)\n";

const ASSERTIONS: &str = r##"
def check(label, got, want)
  raise "#{label}: got #{got.inspect}, want #{want.inspect}" unless got == want
end
a = Article.create!(title: "Sliced", body: "A sufficiently long body.")
check("typed call site", a.summary, { "id" => a.id, "title" => "Sliced" })
check("untyped call site", Article.all.first.slice(:title, :body), { "title" => "Sliced", "body" => "A sufficiently long body." })
check("no names", a.slice, {})
begin
  a.slice(:id, :comments)
  raise "a name that is not a column answered"
rescue NotImplementedError => e
  check("non-column message", e.message.include?("comments"), true)
end
puts "slice passed"
"##;

fn sliced_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog().edit(
        "app/models/article.rb",
        "class Article < ApplicationRecord\n",
        &format!("class Article < ApplicationRecord\n{SUMMARY}"),
    )
}

#[test]
fn a_records_slice_answers_its_named_columns() {
    let run = sliced_app().run_ruby(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("slice passed"));
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn a_records_slice_answers_its_named_columns_on_spinel() {
    let run = sliced_app().run_spinel_with_db(ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("slice passed"));
}
