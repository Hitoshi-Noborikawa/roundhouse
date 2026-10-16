//! `json.extract!(record, *attrs)` / `json.(record, *attrs)`, and
//! `json.array!(collection, *attrs)`.
//!
//! Jbuilder's `_extract_method_values` writes `record.public_send(attr)`
//! under each name exactly as given, Symbol or String, however the list
//! was built. The lowering accepted only Symbol literals: any other
//! argument (a splat, a param, an ivar, a constant, a String literal)
//! classified the statement as unknown, and an unknown statement is
//! written as nothing, so the response was a 200 with the keys missing
//! and nothing reported it. `array!` with attribute names did the same
//! and rendered `{}`.
//!
//! Expected values are Rails 8.1.4 with jbuilder 2.15.1 on the same
//! templates, with row 1 `name: "alpha", size: 3` and row 2
//! `name: "beta", size: 5`.

#[path = "support/emit_and_run.rs"]
mod emit_and_run;

const ACTIONS: &[(&str, &str)] = &[
    ("splat_params", "json.call(@widget, *params[:fields])"),
    ("splat_const", "json.extract!(@widget, *Widget::PUBLIC_ATTRS)"),
    ("splat_ivar", "json.extract!(@widget, *@attrs)"),
    ("single_dynamic", "json.call(@widget, params[:field])"),
    ("mixed", "json.extract!(@widget, :id, *Widget::PUBLIC_ATTRS)"),
    ("string_literal", "json.extract!(@widget, \"name\", \"size\")"),
    ("literal", "json.extract!(@widget, :name, :size)"),
];

fn app() -> emit_and_run::Overlay {
    let mut controller = String::from("class WidgetsController < ApplicationController\n");
    let mut routes = String::from("Rails.application.routes.draw do\n");
    let mut overlay = emit_and_run::empty_app();
    for (action, line) in ACTIONS {
        let ivars = if *action == "splat_ivar" { "\n    @attrs = %i[name size]" } else { "" };
        controller.push_str(&format!(
            "  def {action}\n    @widget = Widget.find(params[:id]){ivars}\n  end\n\n"
        ));
        routes.push_str(&format!(
            "  get \"widgets/:id/{action}\", to: \"widgets#{action}\", defaults: {{ format: :json }}\n"
        ));
        overlay = overlay.write(
            &format!("app/views/widgets/{action}.json.jbuilder"),
            &format!("json.title \"t\"\n{line}\njson.done true\n"),
        );
    }
    controller.push_str("  def index\n    @widgets = Widget.order(:id).to_a\n  end\nend\n");
    routes.push_str("  get \"widgets\", to: \"widgets#index\", defaults: { format: :json }\nend\n");
    overlay
        .write(
            "app/views/widgets/index.json.jbuilder",
            "json.array!(@widgets, :name, :size)\n",
        )
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::API\nend\n",
        )
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/models/widget.rb",
            "class Widget < ApplicationRecord\n  PUBLIC_ATTRS = %i[name size].freeze\nend\n",
        )
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define(version: 1) do\n  create_table :widgets do |t|\n    t.string :name\n    t.integer :size\n  end\nend\n",
        )
        .write("app/controllers/widgets_controller.rb", &controller)
        .write("config/routes.rb", &routes)
}

const PROBE: &str = r##"
def probe(path, query = "")
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path, "QUERY_STRING" => query, "rack.input" => StringIO.new(""))
  text = +""
  body.each { |part| text << part }
  [status, text]
end

def refusal(path, query = "")
  probe(path, query)
  nil
rescue RuntimeError => e
  e.message
end

Widget.create!(name: "alpha", size: 3)
Widget.create!(name: "beta", size: 5)
"##;

/// A list the lowering cannot read at compile time is refused where it
/// stands, the way a non-lowerable `merge!` is, instead of rendering a
/// 200 with the keys missing.
#[test]
fn jbuilder_extract_with_a_dynamic_attribute_list_is_refused() {
    app()
        .run_ruby(&format!(
            r##"{PROBE}
[
  ["/widgets/1/splat_params", "fields[]=name&fields[]=size"],
  ["/widgets/1/splat_const", ""],
  ["/widgets/1/splat_ivar", ""],
  ["/widgets/1/single_dynamic", "field=name"],
  ["/widgets/1/mixed", ""],
].each do |path, query|
  message = refusal(path, query)
  raise "#{{path}} was not refused: #{{probe(path, query).inspect}}" if message.nil?
  raise "#{{path}}: #{{message}}" unless message.include?("extract! with dynamic attributes not supported")
end
got = probe("/widgets/1/literal")
raise "literal #{{got.inspect}}" unless got == [200, %({{"title":"t","name":"alpha","size":3,"done":true}})]
puts "ALL OK"
"##
        ))
        .assert_passes();
}
