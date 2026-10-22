use super::emit_and_run;

/// `uniqueness:` on an attribute naming a `belongs_to`. Rails resolves it
/// to the association's foreign key (`validates :reblog, uniqueness:`
/// queries `"reblog_of_id" = ?`) and keeps the error on `reblog`. The
/// lowered check queried a `reblog` column that does not exist. Answers
/// are Rails 8.1.4's for the same models and rows.
fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write("app/models/account.rb", "class Account < ApplicationRecord\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"accounts\", force: :cascade do |t|\n    t.string \"name\"\n  end\n  create_table \"statuses\", force: :cascade do |t|\n    t.integer \"account_id\"\n    t.integer \"reblog_of_id\"\n    t.boolean \"deleted\", default: false, null: false\n  end\n  create_table \"boosts\", force: :cascade do |t|\n    t.integer \"reblog_of_id\"\n  end\nend\n",
        )
        .write(
            "app/models/status.rb",
            r#"class Status < ApplicationRecord
  belongs_to :account
  belongs_to :reblog, class_name: 'Status', foreign_key: 'reblog_of_id', optional: true

  default_scope { where(deleted: false) }

  validates :reblog, uniqueness: { scope: :account }, if: :reblog?

  def reblog?
    !reblog_of_id.nil?
  end
end
"#,
        )
        .write(
            "app/models/boost.rb",
            "class Boost < ApplicationRecord\n  belongs_to :reblog, class_name: 'Status', foreign_key: 'reblog_of_id', optional: true\n\n  validates :reblog, uniqueness: true\nend\n",
        )
}

#[test]
fn uniqueness_on_a_belongs_to_queries_its_foreign_key_like_rails() {
    app()
        .run_ruby(
            r#"a = Account.create!(name: "a")
b = Account.create!(name: "b")
# An unset association is left unassigned, so its key is nil, as on a
# new Rails record.
def status(account, reblog = nil)
  s = Status.new
  s.account = account
  s.reblog = reblog unless reblog.nil?
  s
end
def errs(record)
  record.valid?
  record.errors.to_a
end
orig = status(a)
orig.save!
other = status(a)
other.save!
status(a, orig).save!
gone = status(b, other)
gone.save!
gone.deleted = true
gone.save!
got = [
  errs(status(a, orig)),
  errs(status(b, orig)),
  errs(status(b, other)),
  errs(status(a)),
]
want = [["Reblog has already been taken"], [], ["Reblog has already been taken"], []]
raise "status errors: #{got.inspect}" unless got == want
Boost.new.save!
nil_boost = Boost.new
boost = Boost.new
boost.reblog = orig
got = [errs(nil_boost), errs(boost)]
raise "boost errors: #{got.inspect}" unless got == [["Reblog has already been taken"], []]
puts "association uniqueness passed"
"#,
        )
        .assert_passes();
}

/// A polymorphic `belongs_to` as the attribute is the reported gap:
/// Rails 8.1.4 raises `ArgumentError` ("Polymorphic associations do not
/// support computing the class.") validating a record that holds one.
#[test]
fn uniqueness_on_a_polymorphic_belongs_to_is_a_reported_gap() {
    let warnings = emit_and_run::empty_app()
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n")
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"pins\", force: :cascade do |t|\n    t.string \"subject_type\"\n    t.integer \"subject_id\"\n  end\nend\n",
        )
        .write(
            "app/models/pin.rb",
            "class Pin < ApplicationRecord\n  belongs_to :subject, polymorphic: true, optional: true\n  validates :subject, uniqueness: true\nend\n",
        )
        .ingest_warnings();
    assert!(
        warnings.iter().any(|w| w.contains("validates :subject, uniqueness:") && w.contains("polymorphic association")),
        "{warnings:?}"
    );
}
