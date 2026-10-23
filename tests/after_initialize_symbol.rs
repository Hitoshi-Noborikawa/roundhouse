//! `after_initialize :method` — the symbol form, with several targets and
//! an `if:` condition — runs on construction and on load, as Rails runs it.
//! Before, only the block form (and a `def after_initialize`) was lowered;
//! the symbol form was a "model DSL call not lowered" warning and the
//! methods never ran. Answers are activerecord 8.1.4's
//! (`rails_answers_the_table`). A bare `Widget.new` with no attributes is
//! #1245 and is not exercised here.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use std::process::Command;

const MODELS: &str = r#"class Widget < ApplicationRecord
  after_initialize :set_salt, :set_name, if: :fresh?

  def seen?
    @seen == true
  end

  private

  def fresh?
    name.nil?
  end

  def set_salt
    self.salt ||= "s"
    @seen = true
  end

  def set_name
    self.name = "n"
  end
end

class Tally < ApplicationRecord
  after_initialize :mark

  def marked?
    @marked == true
  end

  private

  def mark
    @marked = true
  end
end
"#;

/// Ruby run on both sides; each line raises on a mismatch.
const SCRIPT: &str = r#"w = Widget.new(salt: nil)
got = [w.salt, w.name, w.seen?].inspect
raise "new(salt: nil): " + got unless got == %q(["s", "n", true])
u = Widget.new(name: "z")
got = [u.salt, u.name, u.seen?].inspect
raise "new(name: z): " + got unless got == %q([nil, "z", false])
t = Tally.new(label: "x")
raise "tally new: not marked" unless t.marked?
t.save!
loaded = Tally.find(t.id)
raise "tally find: not marked" unless loaded.marked?
puts "PASS"
"#;

const SCHEMA: &str = r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
    t.string "salt"
  end
  create_table "tallies", force: :cascade do |t|
    t.string "label"
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    let (widget, tally) = MODELS.split_once("\nclass Tally").unwrap();
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/widget.rb", &format!("{widget}\n"))
        .write("app/models/tally.rb", &format!("class Tally{tally}"))
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

#[test]
fn the_ruby_target_runs_symbol_after_initialize_callbacks() {
    app().run_ruby(SCRIPT).assert_passes();
}

/// The strict targets take the same `def after_initialize` the block form
/// lowers to, with no emit error.
#[test]
fn strict_targets_emit_the_hook() {
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
  create_table(:widgets) {{ |t| t.string :name; t.string :salt }}
  create_table(:tallies) {{ |t| t.string :label }}
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
{MODELS}
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
