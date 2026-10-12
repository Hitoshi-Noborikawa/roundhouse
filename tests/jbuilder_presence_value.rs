//! `presence` / `present?` / `blank?` in a jbuilder VALUE.
//!
//! ```ruby
//! json.note(widget.note.presence || "none")
//! ```
//!
//! The blank-predicate lowering walks no view body, so the value kept
//! its dynamic `presence` call. The CRuby overlay serves it; the
//! Spinel runtime defines no `String#presence` / `NilClass#presence`,
//! and the partial raised `undefined method 'presence' for nil`. A
//! value is now grounded the way an app body's predicate is
//! (`ActiveSupport.presence(x)` for a receiver with no static type).
//! An `if` condition is left to the view-condition rewrite.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use roundhouse::emit::ruby;
use roundhouse::ingest::ingest_app_from_tree;

const SCHEMA: &str = r#"ActiveRecord::Schema.define do
  create_table "widgets", force: :cascade do |t|
    t.string "name"
    t.string "note"
  end
end
"#;

const ROUTES: &str = r#"Rails.application.routes.draw do
  get "widgets/:id", to: "widgets#show", defaults: { format: :json }
  get "widgets/:id/flag", to: "widgets#flag", defaults: { format: :json }
end
"#;

const CONTROLLER: &str = r#"class WidgetsController < ApplicationController
  def show
    @widget = Widget.find(params[:id])
    render :show
  end

  def flag
    @widget = Widget.find(params[:id])
    render :flag
  end
end
"#;

const SHOW: &str = "json.partial! @widget\n";

const PARTIAL: &str = r#"json.name widget.name
json.note(widget.note.presence || "none")
json.noted widget.note.present?
json.empty widget.note.blank?
"#;

/// A predicate as an `if` condition.
const FLAG: &str = r#"if @widget.name.present?
  json.named true
end
"#;

fn emitted() -> Vec<(String, String)> {
    let files: HashMap<PathBuf, Vec<u8>> = [
        ("db/schema.rb", SCHEMA),
        ("config/routes.rb", ROUTES),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n"),
        ("app/models/widget.rb", "class Widget < ApplicationRecord\nend\n"),
        ("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n"),
        ("app/controllers/widgets_controller.rb", CONTROLLER),
        ("app/views/widgets/show.json.jbuilder", SHOW),
        ("app/views/widgets/_widget.json.jbuilder", PARTIAL),
        ("app/views/widgets/flag.json.jbuilder", FLAG),
    ]
    .iter()
    .map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec()))
    .collect();
    let mut app = ingest_app_from_tree(files).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    ruby::emit_lowered_jbuilder_views(&app)
        .into_iter()
        .map(|f| (f.path.to_string_lossy().into_owned(), f.content))
        .collect()
}

fn view<'a>(files: &'a [(String, String)], suffix: &str) -> &'a str {
    files
        .iter()
        .find(|(p, _)| p.ends_with(suffix))
        .map(|(_, c)| c.as_str())
        .unwrap_or_else(|| panic!("no emitted view ends with {suffix}"))
}

fn partial(files: &[(String, String)]) -> &str {
    view(files, "widgets/_widget_json.rb")
}

#[test]
fn a_predicate_in_a_value_is_grounded() {
    let files = emitted();
    let src = partial(&files);
    for (call, grounded) in [
        ("presence", "ActiveSupport.presence(widget.note)"),
        ("present?", "ActiveSupport.present?(widget.note)"),
        ("blank?", "ActiveSupport.blank?(widget.note)"),
    ] {
        assert!(src.contains(grounded), "{call}:\n{src}");
        assert!(!src.contains(&format!("note.{call}")), "{call}:\n{src}");
    }
}

#[test]
fn a_condition_is_left_to_the_condition_rewrite() {
    let files = emitted();
    let src = view(&files, "widgets/flag_json.rb");
    assert!(src.contains("widget.name.present?"), "{src}");
}

/// The partial on CRuby with the runtime's `JsonBuilder` and
/// `ActiveSupport` helpers (and no ActiveSupport core extensions),
/// against what jbuilder renders for the same rows.
#[test]
fn the_partial_renders_what_jbuilder_renders() {
    let files = emitted();
    let dir = std::env::temp_dir().join(format!("roundhouse-jbuilder-presence-{}", std::process::id()));
    for (path, source) in files.iter().filter(|(p, _)| p.ends_with(".rb")) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, source).unwrap();
    }
    std::fs::write(dir.join("app/views.rb"), "").unwrap();
    let partial = dir.join("app/views/widgets/_widget_json.rb");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("runtime/ruby");
    let script = format!(
        r#"require {json_builder:?}
require {active_support:?}
require {partial:?}
require "json"
Widget = Struct.new(:name, :note)
puts JSON.generate([
  JSON.parse(Views::Widgets.widget_json(Widget.new("a", "hi"))),
  JSON.parse(Views::Widgets.widget_json(Widget.new("b", nil))),
  JSON.parse(Views::Widgets.widget_json(Widget.new("", "  "))),
])
"#,
        json_builder = root.join("json_builder.rb").display().to_string(),
        active_support = root.join("active_support_ext.rb").display().to_string(),
        partial = partial.display().to_string(),
    );
    let output = Command::new("ruby").args(["-e", &script]).output().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = concat!(
        r#"[{"name":"a","note":"hi","noted":true,"empty":false},"#,
        r#"{"name":"b","note":"none","noted":false,"empty":true},"#,
        r#"{"name":"","note":"none","noted":false,"empty":true}]"#,
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), expected);
}
