//! A request constraint is asked with the request once its route's path matches, and a request it
//! refuses goes on to the routes after it, as Rails' router does (#727): a lambda, a guard object
//! and a Hash keyed by a request attribute, on CRuby and on Spinel.

use super::emit_and_run;

const ROUTES: &str = r#"  root "articles#index"
  get "/invite/:code", to: "gates#api", constraints: ->(req) { req.format == :json }
  get "/invite/:code", to: "gates#page"
  constraints subdomain: "admin" do
    get "/panel", to: "gates#admin"
  end
  get "/panel", to: "gates#public_panel"
  constraints(StaffOnly.new) do
    get "/desk", to: "gates#staff"
  end
  get "/desk", to: "gates#visitor"
"#;

const CONTROLLER: &str = r#"class GatesController < ApplicationController
  def api = render(plain: "api")
  def page = render(plain: "page")
  def admin = render(plain: "admin")
  def public_panel = render(plain: "public")
  def staff = render(plain: "staff")
  def visitor = render(plain: "visitor")
end
"#;

const STAFF_ONLY: &str = r#"class StaffOnly
  def matches?(request)
    request.headers["X-Staff"] == "1"
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class GatesControllerTest < ActionDispatch::IntegrationTest
  test "a lambda on the format" do
    get "/invite/abc", headers: { "Accept" => "application/json" }
    assert_equal "api", response.body
    get "/invite/abc"
    assert_equal "page", response.body
  end

  test "a request-keyed Hash" do
    host! "admin.example.com"
    get "/panel"
    assert_equal "admin", response.body
    host! "www.example.com"
    get "/panel"
    assert_equal "public", response.body
  end

  test "a guard object" do
    get "/desk", headers: { "X-Staff" => "1" }
    assert_equal "staff", response.body
    get "/desk"
    assert_equal "visitor", response.body
  end
end
"#;

fn app() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit("config/routes.rb", "  root \"articles#index\"\n", ROUTES)
        .write("app/controllers/gates_controller.rb", CONTROLLER)
        .write("lib/staff_only.rb", STAFF_ONLY)
}

#[test]
fn constraints_are_asked_with_the_request_and_a_refused_one_reaches_the_next_route() {
    app()
        .write("test/controllers/gates_controller_test.rb", TEST)
        .run_test("test/controllers/gates_controller_test.rb")
        .assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn constraints_are_asked_on_spinel() {
    let script = r#"def answer(host, accept, path)
  request = ActionDispatch::Request.new
  request.host = host
  request.format = ActionDispatch::MimeNegotiation.format_name("", accept, "", "")
  matched = RouteTable.match_asking("GET", path, [RouteTable.root] + RouteTable.table, request)
  matched.nil? ? "none" : matched.action.to_s
end
raise "json" unless answer("www.example.com", "application/json", "/invite/abc") == "api"
raise "page" unless answer("www.example.com", "text/html,*/*", "/invite/abc") == "page"
raise "admin" unless answer("admin.example.com", "", "/panel") == "admin"
raise "public" unless answer("www.example.com", "", "/panel") == "public_panel"
raise "visitor" unless answer("www.example.com", "", "/desk") == "visitor"
puts "route guards passed"
"#;
    app().run_spinel(script).assert_passes();
}
