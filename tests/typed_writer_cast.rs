//! #1158: on the ruby-family targets, the public writer of an integer,
//! float, decimal or boolean column casts what it is given, as an
//! ActiveRecord attribute does. Before, `record.min_favs = "5"` read back
//! `"5"`, so a request param reached every validator, comparison and
//! serializer as a String, and `numericality:` called it "is not a
//! number". The table below is what activerecord 8.1.4 answers
//! (`rails_answers_the_cast_table` checks Rails against it); a decimal
//! column is a Float in the lowered app, so its values compare as floats.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use std::process::Command;

const SCHEMA: &str = r#"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "policies", force: :cascade do |t|
    t.integer "min_favs"
    t.float "ratio"
    t.decimal "price"
    t.boolean "active"
  end
end
"#;

const MODEL: &str = r#"class Policy < ApplicationRecord
  validates :min_favs, numericality: { greater_than_or_equal_to: 1 }, allow_nil: true
end
"#;

/// `(value, [min_favs, ratio, price, active])` after `record.col = value`.
const CASTS: &[(&str, &str)] = &[
    ("nil", "[nil, nil, nil, nil]"),
    (r#""""#, "[nil, nil, nil, nil]"),
    (r#"" ""#, "[nil, nil, nil, true]"),
    (r#""abc""#, "[0, 0.0, 0.0, true]"),
    (r#""0""#, "[0, 0.0, 0.0, false]"),
    (r#""5""#, "[5, 5.0, 5.0, true]"),
    (r#"" 5 ""#, "[5, 5.0, 5.0, true]"),
    (r#""5.5""#, "[5, 5.5, 5.5, true]"),
    (r#""1e3""#, "[1, 1000.0, 1000.0, true]"),
    (r#""0x1A""#, "[0, 0.0, 0.0, true]"),
    (r#""1_000""#, "[1000, 1000.0, 1000.0, true]"),
    (r#""f""#, "[0, 0.0, 0.0, false]"),
    (r#""off""#, "[0, 0.0, 0.0, false]"),
    ("true", "[1, 1.0, 1.0, true]"),
    ("false", "[0, 0.0, 0.0, false]"),
    ("5", "[5, 5.0, 5.0, true]"),
    ("5.7", "[5, 5.7, 5.7, true]"),
];

/// `numericality: { greater_than_or_equal_to: 1 }` on a String param, as
/// `Policy.new(min_favs: value)`: the cast value is what is compared.
const NUMERICALITY: &[(&str, &str)] = &[
    (r#""0""#, r#"["Min favs must be greater than or equal to 1"]"#),
    (r#""5""#, "[]"),
    (r#""5.5""#, "[]"),
];

/// Ruby asserting both tables. `$errors` answers a record's full messages,
/// as each side spells it.
fn assertions() -> String {
    let mut s = String::new();
    for (value, want) in CASTS {
        s.push_str(&format!(
            "x = Policy.new\nx.min_favs = {value}\nx.ratio = {value}\nx.price = {value}\nx.active = {value}\ngot = [x.min_favs, x.ratio, x.price.nil? ? nil : x.price.to_f, x.active].inspect\nraise \"{}: #{{got}}\" unless got == {want:?}\n",
            value.replace('"', "\\\"")
        ));
    }
    for (value, want) in NUMERICALITY {
        s.push_str(&format!(
            "x = Policy.new(min_favs: {value})\nx.valid?\ngot = messages(x).inspect\nraise \"numericality {}: #{{got}}\" unless got == {want:?}\n",
            value.replace('"', "\\\"")
        ));
    }
    s
}

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", SCHEMA)
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/policy.rb", MODEL)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

#[test]
fn the_ruby_target_casts_each_typed_writer_as_rails_does() {
    app()
        .run_ruby(&format!("def messages(r)\n  r.errors\nend\n{}puts \"PASS\"\n", assertions()))
        .assert_passes();
}

/// A loaded record keeps the stored values: hydration passes values of the
/// column's type, on which the cast is the identity.
#[test]
fn a_saved_and_reloaded_record_reads_its_stored_values() {
    app()
        .run_ruby(
            r#"x = Policy.new
x.min_favs = "7"
x.ratio = "2.5"
x.price = "9.25"
x.active = "1"
x.save!
y = Policy.find(x.id)
got = [y.min_favs, y.ratio, y.price, y.active].inspect
raise "reloaded: #{got}" unless got == "[7, 2.5, 9.25, true]"
puts "PASS"
"#,
        )
        .assert_passes();
}

/// The tables themselves, against activerecord 8.1.4 on sqlite3.
#[test]
fn rails_answers_the_cast_table() {
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
  create_table(:policies) do |t|
    t.integer :min_favs
    t.float :ratio
    t.decimal :price
    t.boolean :active
  end
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
{MODEL}
def messages(r)
  r.errors.full_messages
end
{asserts}puts "Rails agrees"
"##,
        asserts = assertions(),
    );
    let output = Command::new("ruby").arg("-e").arg(&script).output().expect("ruby is on PATH");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("Rails agrees"),
        "activerecord 8.1.4 disagrees with the table\n=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}"
    );
}
