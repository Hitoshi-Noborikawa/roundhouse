# rails 8.1.4: a strong-params method that is both a before_action target and called by the action.
#
# Run it outside a Rails app's tree:
#
#     ruby tests/before_action_target_also_called/oracle.rb
require "rails"
require "active_record/railtie"
require "action_controller/railtie"
ENV["DATABASE_URL"] = "sqlite3::memory:"
class OracleApp < Rails::Application
  config.eager_load = false
  config.secret_key_base = "x" * 64
  config.logger = Logger.new(nil)
  config.hosts.clear
  config.action_dispatch.show_exceptions = :rescuable
end
OracleApp.initialize!
ActiveRecord::Schema.verbose = false
ActiveRecord::Schema.define { create_table(:gadgets) { |t| t.string :name } }
OracleApp.routes.draw { resources :gadgets, only: :create }
class Gadget < ActiveRecord::Base; end
class GadgetsController < ActionController::Base
  skip_forgery_protection
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
s = ActionDispatch::Integration::Session.new(OracleApp)
s.post "/gadgets", params: { gadget: { name: "z" } }
puts "with gadget: status=#{s.response.status} body=#{s.response.body}"
s.post "/gadgets", params: { other: 1 }
puts "without gadget: status=#{s.response.status}"
puts "gadgets: #{Gadget.count}"
