use super::emit_and_run;

/// Issue #1102: `maximum` / `minimum` / `sum` on a relation with a
/// custom `from(...)` source. Rails 8.1 (`Calculations#aggregate_column`)
/// qualifies the column with the model's table only when the source
/// names that table (`table_name_matches?`), and leaves it bare
/// otherwise, so a subquery source aggregates its own columns:
///
///   Widget.from("(SELECT * FROM widgets WHERE score > 4) AS hi").maximum(:score)
///   # SELECT MAX(score) FROM (SELECT * FROM widgets WHERE score > 4) AS hi  => 7
///
/// An aliased table source (`from("widgets AS recent")`) still names the
/// table, so Rails qualifies with it and the query raises
/// `StatementInvalid` ("no such column: widgets.score"): the build must
/// raise there too, not answer. Answers below are Rails 8.1.4's on the
/// same rows (scores 3, 7, 5).
const MODEL: &str = "class Widget < ApplicationRecord\nend\n";

const APPLICATION_RECORD: &str =
    "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n";

const SCHEMA: &str = r#"ActiveRecord::Schema.define do
  create_table "widgets", force: :cascade do |t|
    t.integer "score"
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", APPLICATION_RECORD)
        .write("db/schema.rb", SCHEMA)
        .write("app/models/widget.rb", MODEL)
}

const ASSERTIONS: &str = r#"[3, 7, 5].each { |s| Widget.create!(score: s) }
hi = "(SELECT * FROM widgets WHERE score > 4) AS hi"
got = {
  plain_max: Widget.maximum(:score),
  plain_min: Widget.minimum(:score),
  plain_sum: Widget.sum(:score).to_i,
  sub_max: Widget.from(hi).maximum(:score),
  sub_min: Widget.from(hi).minimum(:score),
  sub_sum: Widget.from(hi).sum(:score).to_i,
  sub_bare_alias_sum: Widget.from("(SELECT * FROM widgets WHERE score > 4) hi").sum(:score).to_i,
}
want = { plain_max: 7, plain_min: 3, plain_sum: 15, sub_max: 7, sub_min: 5, sub_sum: 12, sub_bare_alias_sum: 12 }
raise "aggregates: #{got.inspect}" unless got == want
aliased = Widget.from("widgets AS recent")
begin
  aliased.maximum(:score)
  raise "aliased maximum answered; Rails raises StatementInvalid"
rescue ActiveRecord::StatementInvalid
end
begin
  aliased.minimum(:score)
  raise "aliased minimum answered; Rails raises StatementInvalid"
rescue ActiveRecord::StatementInvalid
end
begin
  aliased.sum(:score)
  raise "aliased sum answered; Rails raises StatementInvalid"
rescue ActiveRecord::StatementInvalid
end
puts "from aggregates passed"
"#;

#[test]
fn aggregates_over_a_custom_from_source_match_rails() {
    app().run_ruby(ASSERTIONS).assert_passes();
}
