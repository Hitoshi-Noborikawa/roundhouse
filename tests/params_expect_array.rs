#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

fn app() -> emit_and_run::Overlay {
    app_with_receiver("params")
}

fn app_with_receiver(receiver: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 1) do\nend\n",
        )
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :items, only: :create\nend\n",
        )
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::API\nend\n",
        )
        .write(
            "app/controllers/items_controller.rb",
            &format!(
                r#"class ItemsController < ApplicationController
  def create
    {initialization}
    render json: {receiver}.expect(items: [[:header, :other, {{ labels: [] }}]]).map(&:to_h)
  end
end
"#,
                initialization = if receiver == "@params" {
                    "@params = params"
                } else {
                    ""
                }
            ),
        )
}

#[test]
fn array_expect_lowers_explicit_params_receivers() {
    for receiver in ["self.params", "@params"] {
        let (emitted, errors) =
            app_with_receiver(receiver).emit(roundhouse::project::BuildTarget::Spinel);
        assert!(errors.is_empty(), "{receiver}: {errors:?}");
        let source = std::fs::read_to_string(emitted.join("app/controllers/items_controller.rb"))
            .expect("controller");
        assert!(
            source.contains("Params.expect_array("),
            "{receiver}: {source}"
        );
        assert!(!source.contains(".expect("), "{receiver}: {source}");
        app_with_receiver(receiver).run_ruby(r##"require "json"
require "stringio"
raw = JSON.generate({"items" => [{"header" => "H", "ignored" => "removed"}]})
status, _, body = Main.run_rack("REQUEST_METHOD" => "POST", "PATH_INFO" => "/items", "QUERY_STRING" => "", "CONTENT_TYPE" => "application/json", "CONTENT_LENGTH" => raw.bytesize.to_s, "rack.input" => StringIO.new(raw))
raise "#{status} #{body.inspect}" unless status == 200 && JSON.parse(body.join) == [{"header" => "H"}]
puts "PASS"
"##).assert_passes();
    }
}

#[test]
fn array_expect_filters_values_and_refuses_missing_or_malformed_lists() {
    app().run_ruby(r##"require "json"
require "stringio"
cases = [
  [:absent, nil],
  [nil, nil], [false, nil], [1, nil], ["text", nil], [{}, nil], [[], nil], [[1, nil], nil],
  [[{}], [{}]], [[{"ignored" => 1}], [{}]], [[{"header" => ["wrong"]}], [{}]],
  [[{"header" => "H", "other" => "O", "labels" => ["one"], "ignored" => 1}], [{"header" => "H", "other" => "O", "labels" => ["one"]}]],
  [[{"header" => 3, "other" => nil, "labels" => [1, false, nil, "x"]}], [{"header" => 3, "other" => nil, "labels" => [1, false, "x"]}]],
  [[1, {"header" => "H", "labels" => [{"bad" => "value"}]}], [{"header" => "H"}]],
  [[{"labels" => []}, {"header" => false}], [{"labels" => []}, {"header" => false}]]
]
cases.each do |value, expected|
  raw = JSON.generate(value == :absent ? {} : {"items" => value})
  status, _, body = Main.run_rack("REQUEST_METHOD" => "POST", "PATH_INFO" => "/items", "QUERY_STRING" => "", "CONTENT_TYPE" => "application/json", "CONTENT_LENGTH" => raw.bytesize.to_s, "rack.input" => StringIO.new(raw))
  if expected.nil?
    raise "#{value.inspect}: #{status} #{body.inspect}" unless status == 400
  else
    raise "#{value.inspect}: #{status} #{body.inspect}" unless status == 200 && JSON.parse(body.join) == expected
  end
end
puts "PASS"
"##).assert_passes();
}

#[test]
fn array_expect_uses_the_typed_runtime_reader() {
    let (emitted, errors) = app().emit(roundhouse::project::BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:?}");
    let source = std::fs::read_to_string(emitted.join("app/controllers/items_controller.rb"))
        .expect("controller");
    assert!(source.contains("Params.expect_array("), "{source}");
    assert!(!source.contains("@params.expect("), "{source}");
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn array_expect_runs_with_emitted_rbs() {
    app()
        .run_spinel_with_rbs(
            r##"require "json"
controller = ItemsController.new
controller.params = {"items" => [{"header" => "H", "labels" => ["one"], "ignored" => "removed"}]}
controller.process_action(:create)
raise "status" unless controller.status == 200
raise controller.body unless JSON.parse(controller.body) == [{"header" => "H", "labels" => ["one"]}]
begin
  Params.expect_array({"items" => []}, "items", ["header"], ["labels"])
  raise "accepted empty array"
rescue ActionController::ParameterMissing
end
puts "PASS"
"##,
        )
        .assert_passes();
}
