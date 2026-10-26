//! A params helper that names no resource, as lobsters' search and
//! moderation log write them, shared by the Ruby lane and the Spinel lane.

use super::emit_and_run;

const CONTROLLER: &str = r#"class FindsController < ApplicationController
  def index
    @finder = Finder.new(find_params)
    @pairs = @finder.pairs
    @who = log_params.fetch("who", "(All)")
    @stories = log_params.dig(:what, :stories).to_s
    @bogus = log_params.dig(:what, :bogus).to_s
    @page = log_params.fetch(:page) { 1 }.to_i
    @articles = Article.all
    @articles = @articles.joins(:comments).where(comments: { commenter: @who }) unless @who == "(All)"
    @count = @articles.count
  end

  private

  def find_params
    params.permit(:q)
  end

  def log_params
    @log_params ||= params.permit(:who, :page, what: %i[stories comments])
  end
end
"#;

const FINDER: &str = r#"class Finder
  attr_reader :q

  def initialize(params)
    @q = params[:q].to_s
  end

  def pairs
    [{ q: @q }, { page: "one" }]
  end

  def label(value)
    "[#{value}]"
  end
end
"#;

const VIEW: &str = r#"<% @pairs.each do |pair| %>
  <% kind, value = pair.first %>
  <% case kind %>
  <% when :q %>
    <p id="q"><%= @finder.label(value) %></p>
  <% else %>
    <p id="other"><%= value %></p>
  <% end %>
<% end %>
<p id="log"><%= @who %>|<%= @stories %>|<%= @bogus %>|<%= @page %>|<%= @count %></p>
"#;

pub const QUERY: &str = "/finds?q=rails&who=Alice&page=2&what%5Bstories%5D=1&what%5Bbogus%5D=x&extra=y";

pub fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  root \"articles#index\"\n",
            "  root \"articles#index\"\n  resources :finds, only: [:index]\n",
        )
        .write("app/controllers/finds_controller.rb", CONTROLLER)
        .write("app/models/finder.rb", FINDER)
        .write("app/views/finds/index.html.erb", VIEW)
}
