//! A private method that is a `before_action` target and is also called
//! from the action stays defined. `controller_to_library` inlines a
//! controller's own private filter targets into each action and then
//! dropped them, so `before_action :gadget_params` beside
//! `gadget_params[:name]` in the action raised NameError. Rails runs the
//! filter for its `ParameterMissing` check and the action calls it again
//! for the value. Its oracle, on rails 8.1.4:
//!
//!     ruby tests/before_action_target_also_called/oracle.rb

#[path = "support/emit_and_run.rs"]
#[allow(dead_code)]
mod emit_and_run;

const CONTROLLER: &str = r#"class GadgetsController < ApplicationController
  before_action :gadget_params, only: :create

  def create
    @gadget = Gadget.create!(name: gadget_params[:name])
    render json: { name: @gadget.name }
  end

  private

  def gadget_params
    params.require(:gadget).permit(:name)
  end
end
"#;

const TEST: &str = r#"require "test_helper"

class GadgetsControllerTest < ActionDispatch::IntegrationTest
  test "the filter target is still the action's params" do
    post "/gadgets", params: { gadget: { name: "z" } }
    assert_response :success
    assert_equal %({"name":"z"}), response.body
  end
end
"#;

#[test]
fn a_filter_target_the_action_also_calls_stays_defined() {
    emit_and_run::empty_app()
        .write(
            "db/schema.rb",
            "ActiveRecord::Schema.define do\n  create_table \"gadgets\", force: :cascade do |t|\n    t.string \"name\"\n  end\nend\n",
        )
        .write("app/models/application_record.rb", "class ApplicationRecord < ActiveRecord::Base\n  primary_abstract_class\nend\n")
        .write("app/models/gadget.rb", "class Gadget < ApplicationRecord\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/gadgets_controller.rb", CONTROLLER)
        .write("config/routes.rb", "Rails.application.routes.draw do\n  resources :gadgets, only: :create\nend\n")
        .write("test/test_helper.rb", "ENV[\"RAILS_ENV\"] ||= \"test\"\nrequire_relative \"../config/environment\"\nrequire \"rails/test_help\"\n")
        .write("test/controllers/gadgets_controller_test.rb", TEST)
        .run_test("test/controllers/gadgets_controller_test.rb")
        .assert_passes();
}
