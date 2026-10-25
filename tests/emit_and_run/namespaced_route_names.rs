//! A member or collection route's helper puts the action in front of
//! the whole prefix, as Rails' does (#728): `batch_admin_articles_path`
//! in `namespace :admin`, `search_article_comments_path` under
//! `resources :articles`. The app's own calls resolve and answer the
//! paths Rails 8.1.4 gives.

use super::emit_and_run;

#[test]
fn prefixed_member_and_collection_helpers_answer_their_paths() {
    emit_and_run::real_blog()
        .edit(
            "config/routes.rb",
            "  resources :articles do\n    resources :comments, only: [:create, :destroy]\n  end\n",
            concat!(
                "  resources :articles do\n",
                "    resources :comments, only: [:create, :destroy] do\n",
                "      collection { get :search }\n",
                "    end\n",
                "  end\n",
                "  concern :batch do\n",
                "    collection do\n",
                "      post :batch\n",
                "    end\n",
                "  end\n",
                "  namespace :admin do\n",
                "    resources :articles, only: [:index], concerns: :batch do\n",
                "      member { post :approve }\n",
                "    end\n",
                "  end\n",
            ),
        )
        .write(
            "app/controllers/admin/articles_controller.rb",
            r#"class Admin::ArticlesController < ApplicationController
  def index
    article = Article.first
    render plain: [batch_admin_articles_path, approve_admin_article_path(article), search_article_comments_path(article)].join(" ")
  end

  def batch
    head :ok
  end

  def approve
    head :ok
  end
end
"#,
        )
        .write(
            "test/controllers/admin/articles_controller_test.rb",
            r#"require "test_helper"

class Admin::ArticlesControllerTest < ActionDispatch::IntegrationTest
  test "the helpers answer Rails' paths" do
    get "/admin/articles"
    id = Article.first.id
    assert_equal "/admin/articles/batch /admin/articles/#{id}/approve /articles/#{id}/comments/search", response.body
  end
end
"#,
        )
        // The emitted tree flattens the namespaced test's directory.
        .run_test("test/controllers/admin_articles_controller_test.rb")
        .assert_passes();
}
