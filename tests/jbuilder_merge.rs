//! `json.merge!(hash)` merges the hash's pairs into the object.
//!
//! Jbuilder's `merge!` adds each pair of its argument to the object
//! being built. The lowering took `merge!` for a key, so the template
//! wrote `"merge!":"{kind: \"widget\", version: 2}"`. A literal Hash now
//! writes its pairs; any other Hash (an ivar, a method's answer) is
//! walked when the template runs.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

#[test]
fn jbuilder_merge_bang_merges_pairs() {
    emit_and_run::empty_app()
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::API\nend\n")
        .write(
            "app/controllers/gizmos_controller.rb",
            r#"class GizmosController < ApplicationController
  def index
    @extra = { "region" => "north", "count" => params[:n].to_i }
  end

  def show
    @extra = {}
  end
end
"#,
        )
        .write(
            "app/views/gizmos/index.json.jbuilder",
            "json.merge!({ kind: \"widget\", \"version\" => 2 })\njson.merge!(@extra)\njson.done true\n",
        )
        .write("app/views/gizmos/show.json.jbuilder", "json.merge!(@extra)\njson.id 1\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :gizmos, only: %i[index show]\nend\n")
        .run_ruby(
            r##"
def probe(path, query = "")
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => query, "rack.input" => StringIO.new(""))
  text = +""
  body.each { |part| text << part }
  [status, text]
end
got = probe("/gizmos.json", "n=3")
raise "index #{got.inspect}" unless got == [200, %({"kind":"widget","version":2,"region":"north","count":3,"done":true})]
got = probe("/gizmos/1.json")
raise "show #{got.inspect}" unless got == [200, %({"id":1})]
puts "ALL OK"
"##,
        )
        .assert_passes();
}
