use super::emit_and_run;

/// A `widgets` table with an integer `score` and rows 3, 7, 5 and NULL,
/// then `script` with `klass` holding the model: a class held in a
/// variable is the dynamic call site no lowering resolves, so `where`
/// and `find_by` reach the runtime fallbacks and Relation. Every
/// expected value in the scripts below is what Rails 8.1.4 answers on
/// the same rows.
fn run_on_widgets(script: &str) {
    let setup = r##"[3, 7, 5, nil].each do |s|
  w = Widget.new
  w.score = s
  w.save!
end
klass = Widget
def scores(rel)
  rel.to_a.map(&:score).sort_by { |s| s.nil? ? -1 : s }
end
def expect(label, got, want)
  raise "#{label}: got #{got.inspect}, Rails answers #{want.inspect}" unless got == want
end

"##;
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "  create_table \"comments\"",
            "  create_table \"widgets\", force: :cascade do |t|\n    t.integer \"score\"\n  end\n\n  create_table \"comments\"",
        )
        .write("app/models/widget.rb", "class Widget < ApplicationRecord\nend\n")
        .run_ruby(&format!("{setup}{script}"))
        .assert_passes();
}

/// #1129: the class-level `where` fallback forwards every condition
/// form, and `find_by` takes a SQL string, as Rails' do.
#[test]
fn dynamic_where_takes_every_rails_condition_form() {
    run_on_widgets(
        r##"# #1129: the class-level fallback forwards every `where` form.
expect "where(sql)", klass.where("score > 4").count, 2
expect "where(sql, binds)", klass.where("score > ?", 4).count, 2
expect "where(sql, named binds)", klass.where("score > :min", min: 4).count, 2
expect "where(hash)", klass.where(score: 5).count, 1
expect "where(hash) seeds", klass.where(score: 5).find_or_initialize_by(id: -1).score, 5
expect "find_by(sql)", klass.find_by("score > 6").score, 7
expect "find_by(hash)", klass.find_by(score: 3).score, 3
"##,
    );
}

