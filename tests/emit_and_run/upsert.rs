//! `Model.upsert(attrs, on_duplicate: …)` on a table without timestamps,
//! as lobsters' Keystore counts: one statement that inserts, or folds
//! into an UPDATE on the unique key. A table with `created_at` /
//! `updated_at` stays an error: Rails stamps them, the runtime does not.

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
fn upsert_on_a_timestamped_table_stays_an_error() {
    let (_emitted, errors) = emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "  validates :title, presence: true\n",
            "  validates :title, presence: true\n\n  def self.put(id, title)\n    Article.upsert({id: id, title: title, body: \"b\"})\n  end\n",
        )
        .emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("`Article.upsert` on a table with created_at/updated_at")), "{errors:#?}");
}

/// No strict runtime carries `upsert_all`: the call is reported there.
#[test]
fn upsert_is_unsupported_on_strict_targets() {
    let (_emitted, errors) = keystore_app().emit(roundhouse::project::BuildTarget::Rust);
    assert!(errors.iter().any(|e| e.contains("`upsert` is the Ruby-family runtime's")), "{errors:#?}");
}
