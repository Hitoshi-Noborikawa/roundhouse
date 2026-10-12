//! `record.update!(<x>_params)` when no model is named after `<x>`
//! (#856).
//!
//! The call site was renamed to the typed variant
//! `update_from_<x>_params!` whenever the helper's permit list was
//! recognized, but the model lowering defines that variant only on the
//! model the resource names. A record wrapped under another key
//! (`params.require(:gadget)` updating a `Widget`) got a call to a
//! method nothing defines: `NoMethodError` where Rails runs the update.
//! Now the rename needs a model to define the variant; otherwise the
//! call stays Rails' `update!`, handed the permitted attributes as a
//! Hash.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{target_files, BuildTarget};

const SCHEMA: &str = "ActiveRecord::Schema.define(version: 1) do\n  \
    create_table :widgets do |t|\n    t.string :name\n    t.string :color\n  end\nend\n";

/// Every emitted file whose path contains `needle`, for an `update`
/// action that hands `<key>_params` (permitting `:name`, `:color`) to
/// `@widget.update!`.
fn emitted(target: BuildTarget, key: &str, needle: &str) -> String {
    let controller = format!(
        "class WidgetsController < ApplicationController\n  def update\n    @widget = Widget.find(params[:id])\n    @widget.update!({key}_params)\n    head :no_content\n  end\n\n  private\n\n  def {key}_params\n    params.require(:{key}).permit(:name, :color)\n  end\nend\n"
    );
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(PathBuf::from("app/models/widget.rb"), b"class Widget < ApplicationRecord\nend\n".to_vec());
    tree.insert(
        PathBuf::from("config/routes.rb"),
        b"Rails.application.routes.draw do\n  resources :widgets, only: :update\nend\n".to_vec(),
    );
    tree.insert(PathBuf::from("app/controllers/widgets_controller.rb"), controller.into_bytes());
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    let files = target_files(&app, Path::new("fixtures/tiny-blog"), target).expect("emit");
    files
        .into_iter()
        .filter(|(p, _)| p.contains(needle))
        .map(|(p, c)| format!("# {p}\n{c}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_resource_with_no_model_keeps_rails_update() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let src = emitted(target, "gadget", "widgets_controller");
        assert!(!src.contains("update_from_gadget_params"), "{target:?}:\n{src}");
        assert!(src.contains("@widget.update!(self.gadget_params.to_attrs)"), "{target:?}:\n{src}");
        let params = emitted(target, "gadget", "gadget_params");
        assert!(params.contains("def to_attrs"), "{target:?}: the params class answers the Hash:\n{params}");
    }
}

/// The model the resource names still gets its typed variant.
#[test]
fn the_models_own_resource_is_still_renamed() {
    for target in [BuildTarget::Ruby, BuildTarget::Spinel] {
        let src = emitted(target, "widget", "widgets_controller");
        assert!(src.contains("@widget.update_from_widget_params!(self.widget_params)"), "{target:?}:\n{src}");
        let model = emitted(target, "widget", "models/widget.rb");
        assert!(model.contains("def update_from_widget_params!"), "{target:?}:\n{model}");
    }
}
