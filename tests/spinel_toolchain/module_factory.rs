use super::emit_and_run;

const FACTORY: &str = r#"module CopyLog
  def self.[](*cols)
    Module.new do
      extend ActiveSupport::Concern

      included do
        after_create do |record|
          table_name = "#{record.class.table_name}_log"
          column_list = cols.join(", ")
          value_list = (["?"] * cols.length).join(", ")
          values = cols.map { |c| record.public_send(c) }
          ActiveRecord::Base.connection.exec_insert("INSERT INTO #{table_name} (rowid, #{column_list}) values (?, #{value_list})", nil, [record.id] + values)
        end
      end
    end
  end
end
"#;

/// The emit_and_run module factory contract (`include CopyLog[:title, :body]`
/// expanded, its one-parameter `after_create` block reading the record),
/// compiled by Spinel, which cannot build the factory's `Module.new`.
#[test]
#[ignore = "requires the Spinel toolchain, run in its CI lane"]
fn an_included_module_factory_runs_natively() {
    let run = emit_and_run::real_blog()
        .write("app/models/copy_log.rb", FACTORY)
        .edit(
            "db/schema.rb",
            "  create_table \"comments\"",
            "  create_table \"articles_log\", id: false, force: :cascade do |t|\n    t.integer \"rowid\"\n    t.string \"title\"\n    t.text \"body\"\n  end\n\n  create_table \"comments\"",
        )
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  include CopyLog[:title, :body]\n\n  def self.logged\n    ActiveRecord::Base.connection.select_all(\"SELECT rowid, title, body FROM articles_log ORDER BY rowid\").to_a.map { |r| [r[\"title\"], r[\"body\"]] }\n  end\n",
        )
        .run_spinel_with_db(r#"Article.delete_all
Article.create!(title: "Hello", body: "A sufficiently long body.")
raise "logged: #{Article.logged.inspect}" unless Article.logged == [["Hello", "A sufficiently long body."]]
puts "module factory passed"
"#);
    run.assert_passes();
    assert!(run.stdout.contains("module factory passed"));
}
