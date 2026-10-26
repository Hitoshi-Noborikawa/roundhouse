use super::bare_permit_contract as contract;

const TEST: &str = r#"require "test_helper"

class FindsControllerTest < ActionDispatch::IntegrationTest
  test "a bare permit keeps the listed keys and reads them indifferently" do
    get "/finds", params: { q: "rails", who: "Alice", page: "2", what: { stories: "1", bogus: "x" }, extra: "y" }
    assert_response :success
    assert_select "p#q", "[rails]"
    assert_select "p#other", "one"
    assert_select "p#log", "Alice|1||2|1"
  end

  test "absent keys take the fallbacks" do
    get "/finds"
    assert_response :success
    assert_select "p#q", "[]"
    assert_select "p#log", "(All)|||1|2"
  end
end
"#;

/// `params.permit(:q)` handed to a plain class, `@x ||= params.permit(:who,
/// what: %i[...])` read with String and Symbol keys, a multi-local
/// assignment and a `case` over ivars in the template, and a relation ivar
/// joined by association name; each answers what Rails 8.1 answers.
#[test]
fn a_bare_permit_runs_after_emission() {
    contract::overlay()
        .write("test/controllers/finds_controller_test.rb", TEST)
        .run_test("test/controllers/finds_controller_test.rb")
        .assert_passes();
}
