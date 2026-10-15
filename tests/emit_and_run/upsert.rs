//! `Model.upsert(attrs, on_duplicate: …)`, as lobsters' Keystore counts:
//! one statement that inserts, or folds into an UPDATE on the unique key.
//! On a table with timestamps it stamps them as Rails 8.1 does: both on
//! insert, `updated_at` again only when an assigned column changed.

use super::emit_and_run;

fn keystore_app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "db/schema.rb",
            "create_table \"comments\", force: :cascade do |t|\n",
            "create_table \"keystores\", force: :cascade do |t|\n    t.string \"key\", null: false\n    t.bigint \"value\"\n    t.index [\"key\"], name: \"index_keystores_on_key\", unique: true\n  end\n\n  create_table \"comments\", force: :cascade do |t|\n",
        )
        .write(
            "app/models/keystore.rb",
            "class Keystore < ApplicationRecord\n  def self.value_for(key)\n    where(key: key).pick(:value)\n  end\n\n  def self.put(key, value)\n    Keystore.upsert({key: key, value: value}, unique_by: :key, returning: false)\n    true\n  end\n\n  def self.incremented_value_for(key, amount = 1)\n    Keystore.transaction do\n      Keystore.upsert({key: key, value: amount}, unique_by: :key, on_duplicate: Arel.sql(\"value = value + 1\"))\n      value_for(key)\n    end\n  end\nend\n",
        )
        .write(
            "test/models/keystore_test.rb",
            r#"require "test_helper"

class KeystoreTest < ActiveSupport::TestCase
  test "upsert inserts, then folds into an update on the key" do
    assert Keystore.put("hits", 5)
    assert_equal 5, Keystore.value_for("hits")
    Keystore.put("hits", 7)
    assert_equal 7, Keystore.value_for("hits")
    assert_equal 8, Keystore.incremented_value_for("hits")
    assert_equal 1, Keystore.incremented_value_for("fresh")
    assert_equal 2, Keystore.count
  end
end
"#,
        )
}

#[test]
fn upsert_inserts_then_updates_on_the_unique_key() {
    keystore_app().run_test("test/models/keystore_test.rb").assert_passes();
}

#[test]
fn upsert_stamps_timestamps_as_rails_does() {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def self.put(id, title)\n    Article.upsert({id: id, title: title, body: \"b\"})\n  end\n",
        )
        .write(
            "test/models/article_upsert_test.rb",
            r#"require "test_helper"

class ArticleUpsertTest < ActiveSupport::TestCase
  test "timestamps are stamped on insert and moved only by a change" do
    Article.put(500, "First")
    stamped = Article.find(500)
    assert stamped.created_at.present?
    assert_equal stamped.created_at, stamped.updated_at
    sleep 0.01
    Article.put(500, "First")
    unchanged = Article.find(500)
    assert_equal stamped.updated_at, unchanged.updated_at
    sleep 0.01
    Article.put(500, "Second")
    changed = Article.find(500)
    assert_equal "Second", changed.title
    assert_equal stamped.created_at, changed.created_at
    assert changed.updated_at > stamped.updated_at
    Article.upsert({"id" => 501, "title" => "Given", "body" => "b", "created_at" => "2020-01-01 00:00:00", "updated_at" => "2020-01-01 00:00:00"})
    given = Article.find(501)
    assert_equal "Given", given.title
    assert_equal 2020, given.created_at.year
    Article.upsert({"id" => 501, "title" => "Moved", "body" => "b", "updated_at" => "2021-01-01 00:00:00"})
    assert_equal 2021, Article.find(501).updated_at.year
  end
end
"#,
        )
        .run_test("test/models/article_upsert_test.rb")
        .assert_passes();
}

/// No strict runtime carries `upsert_all`: the call is reported there.
#[test]
fn upsert_is_unsupported_on_strict_targets() {
    let (_emitted, errors) = keystore_app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(errors.iter().any(|e| e.contains("`upsert` is the Ruby-family runtime's")), "{errors:#?}");
}
