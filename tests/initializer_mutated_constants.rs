//! A constant an initializer appends to, replaces or reassigns (#1256).
//! Rails runs `config/initializers` before it loads models and draws
//! routes, so `validates :kind, inclusion: { in: Catalog::KINDS }` and
//! `constraints: { id: Regexp.union(Catalog::KINDS) }` read the value the
//! initializer left (`rails_reads_the_initializers_value`, activerecord
//! and actionpack 8.1.4). The constant fold read only the inputs, so it
//! emitted the original list. A boot source is now observed before the
//! folds: a constant it writes or mutates doesn't fold, and the
//! validation and the constraint are reported gaps. An initializer that
//! only reads the constant, defines a top-level constant of its own, or
//! calls the owner, leaves the fold alone.

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

use std::process::Command;

const CATALOG: &str = "module Catalog\n  KINDS = %w[news blog]\nend\n";
const WIDGET: &str = "class Widget < ApplicationRecord\n  validates :kind, inclusion: { in: Catalog::KINDS }\nend\n";
const KINDS_CONTROLLER: &str = "class KindsController < ApplicationController\n  def show\n    render plain: params[:id]\n  end\nend\n";
const ROUTE: &str = "  get \"/kinds/:id\", to: \"kinds#show\", constraints: { id: Regexp.union(Catalog::KINDS) }\n";

/// Each initializer that leaves `Catalog::KINDS` other than `%w[news blog]`.
const MUTATING: [&str; 4] = [
    "Catalog::KINDS << \"wiki\"\n",
    "Catalog.send(:remove_const, :KINDS)\nCatalog::KINDS = %w[wiki]\n",
    "Catalog::KINDS.replace(%w[wiki])\n",
    "Catalog.send(:remove_const, :KINDS)\nCatalog.const_set(:KINDS, %w[wiki])\n",
];

/// Reads the constant and defines a top-level one of its own.
const READING: &str = "LIMIT = 5\nKIND_COUNT = Catalog::KINDS.size\n";

/// Calls the owner without touching its constant: a call on a class or
/// module is not a write to one of its constants.
const CALLING: &str = "Catalog.class_eval do\n  def self.label = \"catalog\"\nend\nCatalog.respond_to?(:label)\n";

fn app(initializer: Option<&str>, route: bool) -> emit_and_run::Overlay {
    let routes = format!("Rails.application.routes.draw do\n{}end\n", if route { ROUTE } else { "" });
    let app = emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define(version: 1) do\n  create_table \"widgets\", force: :cascade do |t|\n    t.string \"kind\"\n  end\nend\n")
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/lib/catalog.rb", CATALOG)
        .write("app/models/widget.rb", WIDGET)
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/kinds_controller.rb", KINDS_CONTROLLER)
        .write("config/routes.rb", &routes);
    match initializer {
        Some(source) => app.write("config/initializers/catalog.rb", source),
        None => app,
    }
}

fn model(initializer: Option<&str>) -> String {
    let (emitted, errors) = app(initializer, false).emit(roundhouse::project::BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    std::fs::read_to_string(emitted.join("app/models/widget.rb")).expect("widget.rb")
}

#[test]
fn a_mutated_constant_does_not_fold_into_a_validation() {
    for initializer in MUTATING {
        let widget = model(Some(initializer));
        assert!(!widget.contains("[\"news\", \"blog\"]"), "{initializer}\n{widget}");
        let warnings = app(Some(initializer), false).ingest_warnings();
        assert!(
            warnings.iter().any(|w| w.contains("validates :kind, inclusion:") && w.contains("not supported")),
            "{initializer}: {warnings:#?}"
        );
    }
}

#[test]
fn a_mutated_constant_does_not_fold_into_a_route_constraint() {
    for initializer in MUTATING {
        let refused = std::panic::catch_unwind(|| app(Some(initializer), true).ingest_warnings());
        let message = refused.expect_err("the constraint is refused").downcast::<String>().map(|m| *m).unwrap_or_default();
        assert!(message.contains("constraints: id"), "{initializer}: {message}");
    }
}

#[test]
fn an_initializer_that_only_reads_keeps_the_fold() {
    for initializer in [None, Some(READING), Some(CALLING)] {
        assert!(model(initializer).contains("[\"news\", \"blog\"].include?(@kind)"), "{initializer:?}");
        let (emitted, errors) = app(initializer, true).emit(roundhouse::project::BuildTarget::Ruby);
        assert!(errors.is_empty(), "{errors:#?}");
        let routes = std::fs::read_to_string(emitted.join("config/routes.rb")).expect("routes.rb");
        assert!(routes.contains("news") && routes.contains("blog"), "{initializer:?}\n{routes}");
    }
}

/// Rails, with each initializer applied before the model is defined and
/// the routes are drawn: the validation and the constraint read the
/// initializer's value, which the original list does not match.
#[test]
fn rails_reads_the_initializers_value() {
    let probe = r#"gem "activerecord", "8.1.4"; gem "actionpack", "8.1.4"; require "active_record"; require "action_controller"; require "sqlite3""#;
    if !Command::new("ruby").args(["-e", probe]).status().is_ok_and(|s| s.success()) {
        eprintln!("skipping: ruby with activerecord/actionpack 8.1.4 and sqlite3 not available");
        return;
    }
    let expected = [("news", true, true), ("news", false, false), ("news", false, false), ("news", false, false)];
    for (initializer, (kind, news_valid, news_routed)) in MUTATING.iter().zip(expected) {
        let script = format!(
            r##"gem "activerecord", "8.1.4"
gem "actionpack", "8.1.4"
require "active_record"
require "action_controller"
require "rack/mock"
require "sqlite3"
ActiveRecord::Base.establish_connection(adapter: "sqlite3", database: ":memory:")
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define do
  create_table(:widgets) {{ |t| t.string :kind }}
end
class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
class ApplicationController < ActionController::Base; end
{CATALOG}
{initializer}
{WIDGET}
{KINDS_CONTROLLER}
routes = ActionDispatch::Routing::RouteSet.new
routes.draw do
{ROUTE}end
status = ->(id) {{ routes.call(Rack::MockRequest.env_for("/kinds/#{{id}}"))[0] }}
got = [Widget.new(kind: "wiki").valid?, Widget.new(kind: {kind:?}).valid?, status.("wiki"), status.({kind:?})]
want = [true, {news_valid}, 200, {news_status}]
raise "Rails: #{{got.inspect}}" unless got == want
puts "PASS""##,
            news_status = if news_routed { 200 } else { 404 },
        );
        let output = Command::new("ruby").arg("-e").arg(&script).output().expect("ruby is on PATH");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stdout.contains("PASS"),
            "{initializer}: activerecord/actionpack 8.1.4 disagree\n=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}"
        );
    }
}
