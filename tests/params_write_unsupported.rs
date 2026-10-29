//! A write into permitted params (`ps[:tags] = …`) is reported, not left
//! to raise. The permit lowers to a synthesized record with a reader per
//! key and no `[]=`, and lobsters' StoriesController#story_params assigns
//! a Tag relation over its `tags: []` Strings: POST /stories was a
//! NoMethodError on the Ruby and Spinel trees with `check` and the emit
//! both clean. Rails' Parameters accepts the write.

use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::diagnostic::{DiagnosticKind, Severity};
use roundhouse::ingest::ingest_app_from_tree;

fn writes_reported(helper_body: &str) -> usize {
    let controller = format!(
        "class PostsController < ApplicationController\n  def create\n    Post.create(post_params)\n    head :ok\n  end\n\n  private\n\n  def post_params\n{helper_body}\n  end\nend\n"
    );
    let files: Vec<(&str, &str)> = vec![
        ("db/schema.rb", "ActiveRecord::Schema.define do\n  create_table :posts do |t|\n    t.string :title\n  end\nend\n"),
        ("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n"),
        ("app/models/post.rb", "class Post < ApplicationRecord\nend\n"),
        ("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n"),
        ("app/controllers/posts_controller.rb", controller.as_str()),
        ("config/routes.rb", "Rails.application.routes.draw do\n  resources :posts, only: :create\nend\n"),
    ];
    let tree: HashMap<PathBuf, Vec<u8>> = files.iter().map(|(p, c)| (PathBuf::from(p), c.as_bytes().to_vec())).collect();
    let mut app = ingest_app_from_tree(tree).expect("ingest");
    let ((), diags) = roundhouse::emit::diagnostics::scope(|| {
        roundhouse::session::analyze_and_lower(&mut app);
    });
    diags
        .iter()
        .filter(|d| {
            d.severity == Severity::Error
                && matches!(&d.kind, DiagnosticKind::Unsupported { construct, .. } if construct.as_str() == "params write")
        })
        .count()
}

#[test]
fn a_write_into_permitted_params_is_reported() {
    let n = writes_reported("    ps = params.require(:post).permit(:title)\n    ps[:title] = ps[:title].to_s.strip\n    ps");
    assert_eq!(n, 1, "the write is the one unsupported params construct");
}

#[test]
fn reading_permitted_params_reports_nothing() {
    let n = writes_reported("    params.require(:post).permit(:title)");
    assert_eq!(n, 0);
}
