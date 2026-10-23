//! `Model.new(attrs)` / `create(attrs)` assign each key through its
//! writer, the virtual ones included: an `attr_accessor` or `attr_writer`,
//! a `typed_store` attr, the Attributes API, a hand-written `def x=`. The
//! lowered constructor assigned only the columns (and the password,
//! rich-text and attachment keys), so `Member.new(nickname: "n").nickname`
//! was nil where Rails answers "n". `update(attrs)` already routed the
//! virtuals; the constructor now routes the same ones. Answers are
//! activerecord 8.1.4's (`rails_answers_the_table`).

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use std::process::Command;

const MODEL: &str = r##"class Member < ApplicationRecord
  attr_accessor :nickname
  attr_writer :secret_note

  def secret_note_value
    @secret_note
  end

  def badge=(value)
    @badge = "#{value}!"
  end

  def badge
    @badge
  end
end
"##;

const SCRIPT: &str = r##"m = Member.new(email: "x", nickname: "n", secret_note: "s", badge: "b")
got = [m.email, m.nickname, m.secret_note_value, m.badge].inspect
raise "new: " + got unless got == %q(["x", "n", "s", "b!"])
c = Member.create!(email: "a", nickname: "k")
raise "create!: #{c.nickname.inspect}" unless c.nickname == "k" && c.persisted?
c.update(nickname: "u")
raise "update: #{c.nickname.inspect}" unless c.nickname == "u"
# A virtual attribute is not stored: a loaded record has none.
raise "find: #{Member.find(c.id).nickname.inspect}" unless Member.find(c.id).nickname.nil?
raise "omitted: #{Member.new(email: "y").nickname.inspect}" unless Member.new(email: "y").nickname.nil?
puts "PASS"
"##;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table \"members\", force: :cascade do |t|\n    t.string \"email\"\n  end\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/member.rb", MODEL)
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

#[test]
fn the_ruby_target_assigns_virtual_attributes_on_construction() {
    app().run_ruby(SCRIPT).assert_passes();
}

/// The strict targets already compile the same virtual-writer calls in
/// `update`; the constructor adds no field or emit error there.
#[test]
fn strict_targets_emit_the_constructor() {
    use roundhouse::project::BuildTarget;
    for target in [BuildTarget::Rust, BuildTarget::Typescript, BuildTarget::Go] {
        let (_emitted, errors) = app().emit(target);
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
    }
}

/// The script itself, against activerecord 8.1.4 on sqlite3.
#[test]
fn rails_answers_the_table() {
    let probe = r#"gem "activerecord", "8.1.4"; require "active_record"; require "sqlite3""#;
    if !Command::new("ruby").args(["-e", probe]).status().is_ok_and(|s| s.success()) {
        eprintln!("skipping: ruby with activerecord 8.1.4 and sqlite3 not available");
        return;
    }
    let script = format!(
        r##"gem "activerecord", "8.1.4"
require "active_record"
require "sqlite3"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:members) {{ |t| t.string :email }}
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
{MODEL}
{SCRIPT}"##
    );
    let output = Command::new("ruby").arg("-e").arg(&script).output().expect("ruby is on PATH");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("PASS"),
        "activerecord 8.1.4 disagrees\n=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}"
    );
}

/// A writer that isn't public is not a mass-assignment key: Rails'
/// `assign_attributes` calls `public_send`, so `new(role: ...)` with a
/// private `role=` raises `ActiveModel::UnknownAttributeError`. Here an
/// unknown key is ignored, so the private writer is simply not called,
/// by `new` or by `update`. Each way of making it private is covered.
const PRIVATE_MODEL: &str = r##"class Member < ApplicationRecord
  def role_value
    @role
  end

  def tier_value
    @tier
  end

  def rank_value
    @rank
  end

  def tier=(value)
    @tier = value
  end
  private :tier=

  private def rank=(value)
    @rank = value
  end

  private

  def role=(value)
    @role = value
  end
end
"##;

fn private_app() -> emit_and_run::Overlay {
    app().write("app/models/member.rb", PRIVATE_MODEL)
}

#[test]
fn a_private_writer_is_not_a_mass_assignment_key() {
    private_app()
        .run_ruby(
            r##"m = Member.new(email: "x", role: "admin", tier: "gold", rank: 1)
got = [m.email, m.role_value, m.tier_value, m.rank_value].inspect
raise "new: " + got unless got == %q(["x", nil, nil, nil])
m.save!
m.update(role: "admin", tier: "gold", rank: 1)
got = [m.role_value, m.tier_value, m.rank_value].inspect
raise "update: " + got unless got == %q([nil, nil, nil])
puts "PASS"
"##,
        )
        .assert_passes();
}

/// Rails refuses each private writer's key with `UnknownAttributeError`.
#[test]
fn rails_refuses_a_private_writer_key() {
    let probe = r#"gem "activerecord", "8.1.4"; require "active_record"; require "sqlite3""#;
    if !Command::new("ruby").args(["-e", probe]).status().is_ok_and(|s| s.success()) {
        eprintln!("skipping: ruby with activerecord 8.1.4 and sqlite3 not available");
        return;
    }
    let script = format!(
        r##"gem "activerecord", "8.1.4"
require "active_record"
require "sqlite3"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:members) {{ |t| t.string :email }}
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
{PRIVATE_MODEL}
%i[role tier rank].each do |key|
  begin
    Member.new(email: "x", key => "v")
    raise "accepted #{{key}}"
  rescue ActiveModel::UnknownAttributeError
  end
  begin
    Member.create!(email: "x").update(key => "v")
    raise "update accepted #{{key}}"
  rescue ActiveModel::UnknownAttributeError
  end
end
puts "PASS""##
    );
    let output = Command::new("ruby").arg("-e").arg(&script).output().expect("ruby is on PATH");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("PASS"),
        "activerecord 8.1.4 disagrees\n=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}"
    );
}
