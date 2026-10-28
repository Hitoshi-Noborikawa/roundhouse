//! A path segment that one call fills with a String is declared
//! `String`, and an Integer another call passes (a foreign key,
//! `comment.article_id`) reaches it as its `to_s`. Rails renders both
//! the same: `parts_path("1")` and `parts_path(1)` are "/widgets/1/parts".
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const CONTROLLER: &str = "class PartsController < ApplicationController
  def index
    render json: part_paths(current_widget)
  end

  def part_paths(widget_id)
    [widget_parts_path(widget_id), widget_parts_path(widget_id.to_s)]
  end

  private

  def current_widget
    params[:widget_id].to_i
  end
end
";

const ASSERTIONS: &str = r#"
require_relative "app/controllers/parts_controller"

paths = PartsController.new.part_paths(1)
raise paths.inspect unless paths == ["/widgets/1/parts", "/widgets/1/parts"]
puts "part paths passed"
"#;

fn example() -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema.define do\nend\n")
        .write("app/controllers/application_controller.rb", "class ApplicationController < ActionController::Base\nend\n")
        .write("app/controllers/parts_controller.rb", CONTROLLER)
        .write(
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :widgets, only: [] do\n    resources :parts, only: :index\n  end\nend\n",
        )
}

#[test]
fn an_integer_in_a_string_segment_is_stringified_at_the_call_site() {
    let (emitted, _) = example().emit(BuildTarget::Ruby);
    let helpers = std::fs::read_to_string(emitted.join("sig/app/route_helpers.rbs"))
        .or_else(|_| std::fs::read_to_string(emitted.join("app/route_helpers.rbs")))
        .expect("read the emitted helper signatures");
    assert!(helpers.contains("def self.widget_parts_path: (String widget_id) -> String"), "{helpers}");
    let controller = std::fs::read_to_string(emitted.join("app/controllers/parts_controller.rb")).expect("read the emitted controller");
    assert!(
        controller.contains("[RouteHelpers.widget_parts_path(widget_id.to_s), RouteHelpers.widget_parts_path(widget_id.to_s)]"),
        "{controller}"
    );
}

/// The issue's own shape (roundhouse#1468): in a view, a String call and
/// a foreign key read off a record. The view body reaches emit with the
/// foreign key untyped, so the record's integer column is what says so.
#[test]
fn a_foreign_key_in_a_view_is_stringified_at_the_call_site() {
    let (emitted, _) = emit_and_run::real_blog()
        .edit(
            "app/views/articles/show.html.erb",
            "py-2 rounded\" %>\n<% end %>\n",
            "py-2 rounded\" %>\n<% end %>\n<%= link_to \"Comments\", article_comments_path(@article.id.to_s) %>\n",
        )
        .edit(
            "app/views/comments/_comment.html.erb",
            "</div>\n",
            "</div>\n<%= link_to \"All comments\", article_comments_path(comment.article_id) %>\n",
        )
        .emit(BuildTarget::Spinel);
    let helpers = std::fs::read_to_string(emitted.join("app/route_helpers.rbs")).expect("read the emitted helper signatures");
    assert!(helpers.contains("def self.article_comments_path: (String article_id) -> String"), "{helpers}");
    let partial = std::fs::read_to_string(emitted.join("app/views/comments/_comment.rb")).expect("read the emitted partial");
    assert!(partial.contains("RouteHelpers.article_comments_path(comment.article_id.to_s)"), "{partial}");
}

#[test]
fn an_integer_in_a_string_segment_runs_in_ruby() {
    example().run_ruby(ASSERTIONS).assert_passes();
}

#[test]
#[ignore = "requires the Spinel toolchain"]
fn an_integer_in_a_string_segment_runs_on_spinel_with_rbs() {
    example().run_spinel_with_rbs(ASSERTIONS).assert_passes();
}
