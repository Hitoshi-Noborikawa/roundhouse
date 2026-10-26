use super::data_paths_contract as contract;

/// A typed_store attribute validates through its reader, the constructor
/// calls an app's own column writer only for a key it was given,
/// `first_or_initialize` carries the where-conditions, `order` takes an
/// Array, and a framework setting the app reads answers the running
/// environment's value; each as Rails 8.1 answers it.
#[test]
fn lobsters_data_path_constructs_run_after_emission() {
    let run = contract::overlay().run_ruby(contract::ASSERTIONS);
    run.assert_passes();
    assert!(run.stdout.contains("data paths contract passed"), "{}", run.stdout);
}

const TEST: &str = r#"require "test_helper"

class BannerControllerTest < ActionDispatch::IntegrationTest
  test "a computed render local reaches the partial and links by to_param" do
    get articles_url
    assert_response :success
    first = Article.order(:id).first
    assert_select "p#banner a[href=?]", "/articles/a#{first.id}", text: first.title
  end
end
"#;

/// `@above = {partial:, locals: {banner_article: <a call>}}` rendered
/// with `render @above`: the computed local reaches the partial (it was
/// dropped, so lobsters' user stories page raised on nil), and the
/// record links by the model's `to_param`, not its id.
#[test]
fn a_computed_dynamic_render_local_reaches_the_partial() {
    contract::overlay()
        .write("test/controllers/banner_controller_test.rb", TEST)
        .run_test("test/controllers/banner_controller_test.rb")
        .assert_passes();
}
