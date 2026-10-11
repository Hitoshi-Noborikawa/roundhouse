//! The Spinel side of the Rails-parity fixes this file's siblings test
//! on CRuby, in one app and one `spin build`:
//!
//! * a controller's class body reads `Gizmo::Checker::SCOPE`, and the
//!   binary boots (the controller requires `gizmo/checker` before
//!   `app/models` loads, as main already emits);
//! * `request.headers.map { |key, _value| key }` (`tests/request_headers_each.rs`);
//! * `(widget.extras || {}).with_indifferent_access` on a json column
//!   (`tests/indifferent_access_untyped_receiver.rs`);
//! * `widget.parts.loaded?` in a partial when two models declare
//!   `has_many :parts` (`tests/view_has_many_loaded_shared_name.rs`);
//! * `enum :kind, { small: 0, large: 1, unknown: nil }` (`tests/enum_nil_value_label.rs`);
//! * datetime columns in a namespaced partial (`tests/jbuilder_namespaced_view_temporal.rs`).
//!
//! A full `spin build` (the controller/model load order is the
//! server's), so it lives here, not in `SPINEL_TESTS`.

use super::{emit_and_run, native_http};
use roundhouse::project::BuildTarget;

fn app() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("app/controllers/api/v1/widgets_controller.rb", r##"class Api::V1::WidgetsController < ApplicationController
  def index
    @widgets = Widget.all
  end
end
"##)
        .write("app/controllers/application_controller.rb", r##"class ApplicationController < ActionController::API
end
"##)
        .write("app/controllers/widgets_controller.rb", r##"class WidgetsController < ApplicationController
  SCOPE = Gizmo::Checker::SCOPE

  def seed
    widget = Widget.create!(name: "alpha", kind: 1, extras: { "k" => "v" })
    Part.create!(name: "p", widget_id: widget.id)
    Widget.create!(name: "beta")
    render plain: Gizmo::Checker.allowed?(SCOPE).to_s
  end

  def header_names
    names = request.headers.map { |key, _value| key }.select { |key| key.start_with?("HTTP_X_") }
    render plain: names.sort.join(",")
  end

  def extras
    widget = Widget.find(params[:id])
    extras = (widget.extras || {}).with_indifferent_access.merge!({ widget.name => params[:ref] })
    extras.merge!(seen: true)
    render plain: "#{extras.keys.sort.join(",")}|#{extras[:k]}"
  end

  def index
    @widgets = Widget.includes(:parts)
  end
end
"##)
        .write("app/models/application_record.rb", r##"class ApplicationRecord < ActiveRecord::Base
  primary_abstract_class
end
"##)
        .write("app/models/gadget.rb", r##"class Gadget < ApplicationRecord
  has_many :parts
end
"##)
        .write("app/models/part.rb", r##"class Part < ApplicationRecord
  belongs_to :widget, optional: true
  belongs_to :gadget, optional: true
end
"##)
        .write("app/models/widget.rb", r##"class Widget < ApplicationRecord
  has_many :parts
  enum :kind, { small: 0, large: 1, unknown: nil }
end
"##)
        .write("app/services/gizmo/checker.rb", r##"module Gizmo
  class Checker
    SCOPE = "read_widgets"

    def self.allowed?(scope)
      scope == SCOPE
    end
  end
end
"##)
        .write("app/views/api/v1/widgets/_widget.json.jbuilder", r##"json.call(widget, :id, :created_at)
json.touched_at(widget.updated_at)
"##)
        .write("app/views/api/v1/widgets/index.json.jbuilder", r##"json.array!(@widgets) do |widget|
  json.partial!(widget, partial: "api/v1/widgets/widget", as: :widget)
end
"##)
        .write("app/views/widgets/_widget.json.jbuilder", r##"json.call(widget, :id, :name, :kind)
json.preloaded(widget.parts.loaded?)
"##)
        .write("app/views/widgets/index.json.jbuilder", r##"json.array!(@widgets) do |widget|
  json.partial!(widget, partial: "widgets/widget", as: :widget)
end
"##)
        .write("config/routes.rb", r##"Rails.application.routes.draw do
  namespace :api do
    namespace :v1 do
      resources :widgets, only: [:index]
    end
  end
  get "/widgets/seed", to: "widgets#seed"
  get "/widgets/header_names", to: "widgets#header_names"
  get "/widgets/:id/extras", to: "widgets#extras"
  resources :widgets, only: [:index]
end
"##)
        .write("db/schema.rb", r##"ActiveRecord::Schema[8.1].define(version: 1) do
  create_table "widgets", force: :cascade do |t|
    t.string "name", null: false
    t.integer "kind"
    t.json "extras", default: {}
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end
  create_table "gadgets", force: :cascade do |t|
    t.string "name", null: false
  end
  create_table "parts", force: :cascade do |t|
    t.string "name", null: false
    t.integer "widget_id"
    t.integer "gadget_id"
  end
end
"##)
}

#[test]
#[ignore = "requires Spinel; run in its CI lane"]
fn rails_parity_fixes_run_on_spinel() {
    let (tree, errors) = app().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "analysis/emission errors:\n{}", errors.join("\n"));

    native_http::build(&tree);
    let server = native_http::Server::start(&tree);

    let seed = server.get("/widgets/seed");
    assert_eq!((seed.status, seed.body.as_str()), (200, "true"), "seed:\n{}", server.log());

    let names = server.get_with_headers("/widgets/header_names", &[("X-Trace", "1"), ("X-Other", "2")]);
    assert_eq!(names.body, "HTTP_X_OTHER,HTTP_X_TRACE", "headers:\n{}", server.log());

    let extras = server.get("/widgets/1/extras?ref=r-9");
    assert_eq!(extras.body, "alpha,k,seen|v", "indifferent:\n{}", server.log());

    let index = server.get("/widgets.json");
    assert_eq!(
        index.body,
        r#"[{"id":1,"name":"alpha","kind":"large","preloaded":true},{"id":2,"name":"beta","kind":"unknown","preloaded":true}]"#,
        "loaded? and enum:\n{}",
        server.log()
    );

    let namespaced = server.get("/api/v1/widgets.json");
    let stamps: Vec<&str> = namespaced.body.split('"').filter(|s| s.len() == 24 && s.ends_with('Z')).collect();
    assert_eq!(stamps.len(), 4, "xmlschema(3) timestamps: {}\n{}", namespaced.body, server.log());
    assert!(
        stamps.iter().all(|s| s.as_bytes()[10] == b'T' && s.as_bytes()[19] == b'.'),
        "xmlschema(3) timestamps: {}",
        namespaced.body
    );
}
