//! `ActiveModel::Serializer` - a bounded subset of active_model_serializers
//! 0.10, expanded before inference into plain methods
//! (`ingest::active_model_serializer`), with its JSON writer settled after
//! it (`lower::as_json_poro`). The behavior against the gem's own output is
//! pinned in `emit_and_run.rs`; these pin the shape and the declines.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::ingest::{ingest_app_from_tree, survey, IngestError};

const SCHEMA: &str = "ActiveRecord::Schema.define(version: 1) do\n  \
    create_table :articles do |t|\n    t.string :title\n    t.datetime :published_at\n  end\nend\n";

fn tree(serializer: &str, controller: &str) -> HashMap<PathBuf, Vec<u8>> {
    let mut tree: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    tree.insert(PathBuf::from("db/schema.rb"), SCHEMA.as_bytes().to_vec());
    tree.insert(PathBuf::from("app/models/article.rb"), b"class Article < ApplicationRecord\nend\n".to_vec());
    tree.insert(
        PathBuf::from("config/routes.rb"),
        b"Rails.application.routes.draw do\n  resources :articles, only: [:show, :index]\nend\n".to_vec(),
    );
    tree.insert(PathBuf::from("app/serializers/article_serializer.rb"), serializer.as_bytes().to_vec());
    tree.insert(PathBuf::from("app/controllers/articles_controller.rb"), controller.as_bytes().to_vec());
    tree
}

const SERIALIZER: &str = "class ArticleSerializer < ActiveModel::Serializer\n  attributes :id, :title, :published_at\nend\n";

const CONTROLLER: &str = r#"class ArticlesController < ApplicationController
  def show
    render json: Article.find(params[:id]), serializer: ArticleSerializer
  end
end
"#;

/// The emitted serializer and controller, and the lowering's diagnostics.
fn emitted(serializer: &str, controller: &str) -> (String, String, Vec<String>) {
    let mut app = ingest_app_from_tree(tree(serializer, controller)).expect("ingest");
    let diags = roundhouse::session::analyze_and_lower(&mut app);
    let (files, emit_diags) = roundhouse::emit::diagnostics::scope(|| {
        roundhouse::project::target_files(&app, std::path::Path::new("fixtures/tiny-blog"), roundhouse::project::BuildTarget::Ruby)
    });
    let files = files.expect("emit");
    let find = |needle: &str| {
        files.iter().filter(|(p, _)| p.ends_with(needle)).map(|(_, c)| c.clone()).collect::<Vec<_>>().join("\n")
    };
    let messages = diags.into_iter().chain(emit_diags).map(|d| d.message).collect();
    (find("article_serializer.rb"), find("articles_controller.rb"), messages)
}

/// The serializer loses its gem parent and gains its readers and writer;
/// the render calls the writer, and a temporal column is written as Rails
/// writes it.
#[test]
fn the_serializer_becomes_a_plain_class_with_a_writer() {
    let (serializer, controller, _) = emitted(SERIALIZER, CONTROLLER);
    assert!(!serializer.contains("ActiveModel::Serializer"), "{serializer}");
    for needle in ["def ams_title", "def serializable_hash", "def as_json_str", "json_time"] {
        assert!(serializer.contains(needle), "{needle}:\n{serializer}");
    }
    assert!(controller.contains("as_json_str"), "{controller}");
    assert!(!controller.contains("serializer:"), "{controller}");
}

/// A declaration outside the subset fails the class: never lowered in
/// part, and survey mode ledgers it.
#[test]
fn a_declaration_outside_the_subset_is_refused() {
    let serializer = "class ArticleSerializer < ActiveModel::Serializer\n  attributes :id\n  has_one :author\nend\n";
    assert!(ingest_app_from_tree(tree(serializer, CONTROLLER)).is_err());
    survey::activate();
    let result = ingest_app_from_tree(tree(serializer, CONTROLLER));
    let gaps = survey::drain();
    let app = result.expect("survey ingest recovers");
    assert!(
        gaps.iter().any(|g| matches!(g, IngestError::Unsupported { message, .. }
            if message.contains("ActiveModel::Serializer subset") && message.contains("`has_one`"))),
        "{gaps:?}"
    );
    let class = app.library_classes.iter().find(|c| c.name.0.as_str() == "ArticleSerializer").expect("class");
    assert!(class.methods.iter().all(|m| m.name.as_str() != "as_json_str"), "lowered in part");
}

/// A render with another serializer option is left as it was, the
/// reported residue.
#[test]
fn a_render_with_another_serializer_option_stays_reported() {
    let controller = "class ArticlesController < ApplicationController\n  def show\n    render json: Article.find(params[:id]), serializer: ArticleSerializer, adapter: :json\n  end\nend\n";
    let (_, controller, messages) = emitted(SERIALIZER, controller);
    assert!(!controller.contains("as_json_str"), "{controller}");
    assert!(messages.iter().any(|m| m.contains("rendering through an active_model_serializers")), "{messages:#?}");
}

/// `render json: record` with no `serializer:`: the gem finds
/// `<Model>Serializer` by name, which this build doesn't, so it's reported.
#[test]
fn an_implicit_serializer_render_is_reported() {
    let controller = "class ArticlesController < ApplicationController\n  def show\n    render json: Article.find(params[:id])\n  end\nend\n";
    let (_, _, messages) = emitted(SERIALIZER, controller);
    assert!(messages.iter().any(|m| m.contains("through `ArticleSerializer`, found by its name")), "{messages:#?}");
}

/// A serializer another app serializer inherits from is a base: lowered,
/// it would lose the gem parent its subclass still needs (`attributes`,
/// `as_json`). It and its subclass stay as they were.
#[test]
fn a_serializer_base_stays_as_it_was() {
    let mut tree = tree(
        "class ArticleSerializer < ApplicationSerializer\n  attributes :id, :title\nend\n",
        CONTROLLER,
    );
    tree.insert(
        PathBuf::from("app/serializers/application_serializer.rb"),
        b"class ApplicationSerializer < ActiveModel::Serializer\nend\n".to_vec(),
    );
    let app = ingest_app_from_tree(tree).expect("ingest");
    let base = app.library_classes.iter().find(|c| c.name.0.as_str() == "ApplicationSerializer").expect("base");
    assert_eq!(base.parent.as_ref().map(|p| p.0.as_str()), Some("ActiveModel::Serializer"));
    assert!(base.methods.iter().all(|m| m.name.as_str() != "as_json_str"), "the base was lowered");
}
