use super::emit_and_run;

/// `default_scope { where(col: literal) }` applies to every query on the
/// model, as Rails' does: class finders and terminals, association
/// readers, bulk writes. It is applied when a relation is built, so
/// `unscoped`, `unscoped { }` and `unscope(where:)` lift it. It was a
/// reported gap, and the emitted tree read and deleted every row
/// (`Gadget.count` was `SELECT COUNT(*) FROM gadgets`; `delete_all` a bare
/// DELETE, #572). Answers are Rails 8.1.4's for the same models and rows.
fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"shelves\", force: :cascade do |t|\n    t.string \"name\"\n  end\n  create_table \"gadgets\", force: :cascade do |t|\n    t.string \"name\"\n    t.boolean \"archived\", default: false, null: false\n    t.integer \"shelf_id\"\n  end\n  create_table \"notes\", force: :cascade do |t|\n    t.integer \"gadget_id\"\n    t.string \"body\"\n  end\n  create_table \"badges\", force: :cascade do |t|\n    t.string \"kind\"\n  end\nend\n",
        )
        .write(
            "app/models/shelf.rb",
            "class Shelf < ApplicationRecord\n  has_many :gadgets\n\n  def self.joined_count\n    joins(:gadgets).count\n  end\n\n  def self.first_with_gadgets\n    Shelf.includes(:gadgets).first\n  end\nend\n",
        )
        .write(
            "app/models/gadget.rb",
            "class Gadget < ApplicationRecord\n  belongs_to :shelf, optional: true\n  default_scope { where(archived: false) }\n\n  def self.clear\n    delete_all\n  end\nend\n",
        )
        .write("app/models/badge.rb", "class Badge < ApplicationRecord\n  default_scope { where(kind: \"basic\") }\nend\n")
        .write(
            "app/models/note.rb",
            "class Note < ApplicationRecord\n  belongs_to :gadget, optional: true\n\n  def self.first_with_gadget\n    Note.includes(:gadget).first\n  end\nend\n",
        )
}

const SEED: &str = r#"shelf = Shelf.create!(name: "s")
live = Gadget.create!(name: "live", shelf_id: shelf.id)
gone = Gadget.create!(name: "gone", shelf_id: shelf.id)
gone.archived = true
gone.save!
note = Note.create!(gadget_id: gone.id, body: "n")
"#;

#[test]
fn reads_on_a_default_scoped_model_apply_the_scope_like_rails() {
    app()
        .run_ruby(&format!(
            r#"{SEED}missing = begin
  Gadget.find(gone.id)
  "found"
rescue ActiveRecord::RecordNotFound
  "not found"
end
got = [
  Gadget.count,
  Gadget.order(:name).map(&:name),
  Gadget.pluck(:name),
  Gadget.where(name: "gone").count,
  missing,
  Gadget.find(live.id).name,
  Gadget.find_by(id: gone.id).nil?,
  Gadget.exists?(name: "gone"),
  Gadget.first.name,
  Gadget.last.name,
  Gadget.where(archived: true).count,
  shelf.gadgets.count,
  Note.find(note.id).gadget.nil?,
  Shelf.joined_count,
  Shelf.first_with_gadgets.gadgets.size,
  Note.first_with_gadget.gadget.nil?,
]
want = [1, ["live"], ["live"], 0, "not found", "live", true, false, "live", "live", 0, 1, true, 1, 1, true]
raise "reads: #{{got.inspect}}" unless got == want
puts "default scope reads passed"
"#
        ))
        .assert_passes();
}

#[test]
fn unscoped_and_unscope_lift_the_default_scope_like_rails() {
    app()
        .run_ruby(&format!(
            r#"{SEED}got = [
  Gadget.unscoped.count,
  Gadget.unscoped {{ Gadget.count }},
  Gadget.unscoped {{ Gadget.where(archived: true).pluck(:name) }},
  Gadget.count,
  Gadget.unscope(where: :archived).count,
  Gadget.unscoped.where(archived: true).count,
  gone.reload.name,
]
want = [2, 2, ["gone"], 1, 2, 1, "gone"]
raise "unscoped: #{{got.inspect}}" unless got == want
puts "default scope unscoped passed"
"#
        ))
        .assert_passes();
}

#[test]
fn new_records_take_the_scope_attributes_like_rails() {
    app()
        .run_ruby(
            r#"fresh = Gadget.new
explicit = Gadget.new(archived: true)
badge = Badge.new
given = Badge.new(kind: "gold")
got = [fresh.archived, explicit.archived, badge.kind, given.kind]
raise "new: #{got.inspect}" unless got == [false, true, "basic", "gold"]
puts "default scope new passed"
"#,
        )
        .assert_passes();
}

#[test]
fn bulk_writes_on_a_default_scoped_model_keep_hidden_rows_like_rails() {
    app()
        .run_ruby(&format!(
            r#"{SEED}Gadget.update_all(name: "u")
renamed = Gadget.unscoped.order(:id).pluck(:name)
Gadget.clear
left = Gadget.unscoped.pluck(:name)
raise "bulk: #{{[renamed, left].inspect}}" unless [renamed, left] == [["u", "gone"], ["gone"]]
puts "default scope bulk writes passed"
"#
        ))
        .assert_passes();
}
