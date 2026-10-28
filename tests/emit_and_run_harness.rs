//! The emit-and-run harness itself: what an overlay carries into the
//! emitted tree, the environment a run gets, and how a refused target
//! reaches the caller.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

/// Not UTF-8 (0x89 cannot start a character), so ingest carries it as a
/// binary asset rather than source, as it does a real PNG.
const PIXEL: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0xff];

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table \"widgets\" do |t|\n    t.string \"name\"\n  end\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
}

#[test]
fn a_run_gets_the_environment_it_is_given() {
    let run = app().run_ruby_env("print ENV.fetch(\"RH_HARNESS_PROBE\")", &[("RH_HARNESS_PROBE", "seen")]);
    run.assert_passes();
    assert_eq!(run.stdout, "seen");
}

#[test]
fn a_binary_asset_reaches_the_emitted_tree_as_the_cli_writes_it() {
    let run = app()
        .write_bytes("public/pixel.png", PIXEL)
        .run_ruby("print File.binread(\"public/pixel.png\").bytes.join(\",\")");
    run.assert_passes();
    let expected = PIXEL.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    assert_eq!(run.stdout, expected);
}

const ENGINE_LOCK: &str = "GEM\n  specs:\n    widget-engine (1.0)\n\nDEPENDENCIES\n  widget-engine\n";
const ENGINE_RBI: &str = "module WidgetEngine\n  class Client\n    sig { returns(String) }\n    def self.label; end\n  end\nend\n";

/// A run of an app whose extension refuses `widget-engine` as Rails
/// dependent, `why` being the YAML for the reason.
fn refused_run(why: &str) -> emit_and_run::Run {
    app()
        .write("roundhouse.yml", "extensions: [config/rh]\n")
        .write("config/rh/gems.yml", &format!("rails_dependent:\n  - gem: widget-engine\n    why: {why}"))
        .write("Gemfile.lock", ENGINE_LOCK)
        .write("sorbet/rbi/gems/widget-engine@1.0.rbi", ENGINE_RBI)
        .write("app/models/report.rb", "class Report\n  def self.value\n    WidgetEngine::Client.label\n  end\nend\n")
        .run_ruby("print Report.value")
}

fn assert_refused(run: &emit_and_run::Run) {
    assert!(run.errors.iter().any(|error| error.contains("widget-engine") && error.contains("requires the Rails framework")), "{:#?}", run.errors);
    assert!(!run.success, "a refused target has no program to run:\n{}", run.stdout);
    assert!(std::fs::read_dir(&run.emitted).unwrap().next().is_none(), "a refused target writes no tree");
}

#[test]
fn a_refused_target_reports_its_errors_instead_of_panicking() {
    assert_refused(&refused_run("requires the Rails framework\n"));
}

#[test]
fn a_refusal_whose_reason_ends_in_a_newline_is_still_reported() {
    // A block scalar keeps its trailing newline, and so does the diagnostic.
    assert_refused(&refused_run("|\n      requires the Rails framework\n"));
}
