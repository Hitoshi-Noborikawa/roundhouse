//! Named request exceptions and inherited `rescue_from` precedence.

use super::emit_and_run;

#[test]
fn parameters_parse_error_resolves_and_reaches_the_bad_request_handler() {
    emit_and_run::empty_app()
        .write(
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::API\nend\n",
        )
        .write(
            "app/controllers/parses_controller.rb",
            r#"class ParsesController < ApplicationController
  rescue_from ActionDispatch::Http::Parameters::ParseError, with: :bad_json

  def show
    raise ActionDispatch::Http::Parameters::ParseError, "bad JSON"
  end

  private

  def bad_json(error)
    render json: { error: error.message }, status: :bad_request
  end
end
"#,
        )
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  get '/parse', to: 'parses#show'\nend\n",
        )
        .run_ruby(
            r#"
raise "exception ancestry" unless ActionDispatch::Http::Parameters::ParseError.superclass == StandardError
status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => "/parse",
                                      "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
text = +""
body.each { |part| text << part }
raise "parse response #{[status, text].inspect}" unless [status, text] == [400, %({"error":"bad JSON"})]
"#,
        )
        .assert_passes();
}

#[test]
fn rescue_from_prefers_the_subclass_then_its_last_declaration() {
    emit_and_run::empty_app()
        .write(
            "app/controllers/application_controller.rb",
            r#"class ApplicationController < ActionController::API
  rescue_from ArgumentError, with: :parent_error
  rescue_from KeyError, with: :missing_key

  private

  def parent_error
    head :bad_request
  end

  def missing_key
    head :not_found
  end
end
"#,
        )
        .write(
            "app/controllers/parents_controller.rb",
            r#"class ParentsController < ApplicationController
  def index
    raise ArgumentError, "parent"
  end
end
"#,
        )
        .write(
            "app/controllers/children_controller.rb",
            r#"class ChildrenController < ParentsController
  rescue_from ArgumentError, with: :earlier_error
  rescue_from ArgumentError, with: :child_error

  def index
    raise ArgumentError, "child"
  end

  def missing
    raise KeyError, "key"
  end

  private

  def earlier_error
    head :unprocessable_entity
  end

  def child_error
    head :conflict
  end
end
"#,
        )
        .write(
            "app/controllers/grandchildren_controller.rb",
            r#"class GrandchildrenController < ChildrenController
  def index
    raise ArgumentError, "grandchild"
  end
end
"#,
        )
        .write(
            "config/routes.rb",
            r#"Rails.application.routes.draw do
  get '/parent', to: 'parents#index'
  get '/child', to: 'children#index'
  get '/grandchild', to: 'grandchildren#index'
  get '/missing', to: 'children#missing'
end
"#,
        )
        .run_ruby(
            r##"
{ "/parent" => 400, "/child" => 409, "/grandchild" => 409, "/missing" => 404 }.each do |path, expected|
  status, _headers, body = Main.run_rack("REQUEST_METHOD" => "GET", "PATH_INFO" => path,
                                        "QUERY_STRING" => "", "rack.input" => StringIO.new(""))
  body.close if body.respond_to?(:close)
  raise "#{path}: #{status}, expected #{expected}" unless status == expected
end
"##,
        )
        .assert_passes();
}
