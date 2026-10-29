//! A key only a `.merge(k: v)` after the permit adds is typed by what the
//! merge writes, and is never read from the request (Rails' permit drops
//! it). lobsters' `tag_params` merges `edit_user_id: @user.id`; the
//! synthesized params class declared it a String, so the Integer the
//! merge assigns was a TypeError on the Spinel tree (POST /mod/tags).

use super::emit_and_run;
use roundhouse::project::BuildTarget;

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/models/article.rb",
            "class Article < ApplicationRecord\n",
            "class Article < ApplicationRecord\n  attr_accessor :editor_id\n",
        )
        .edit(
            "app/controllers/articles_controller.rb",
            "      params.expect(article: [ :title, :body ])\n",
            "      params.expect(article: [ :title, :body ]).merge(editor_id: 7)\n",
        )
}

#[test]
fn a_merged_only_key_takes_the_merged_type_on_spinel() {
    let (emitted, errors) = overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let rbs = std::fs::read_to_string(emitted.join("app/models/article_params.rbs")).expect("rbs");
    assert!(rbs.contains("attr_reader editor_id: Integer?"), "{rbs}");
    let rb = std::fs::read_to_string(emitted.join("app/models/article_params.rb")).expect("rb");
    assert!(!rb.contains("\"editor_id\" =>"), "the String-valued to_h leaves it out:\n{rb}");
}

#[test]
fn a_merged_only_key_is_assigned_as_written_and_not_read_from_the_request() {
    let (emitted, errors) = overlay().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    let controller = std::fs::read_to_string(emitted.join("app/controllers/articles_controller.rb")).expect("controller");
    assert!(controller.contains("_p.editor_id = 7\n"), "the Integer stays an Integer:\n{controller}");
    let rb = std::fs::read_to_string(emitted.join("app/models/article_params.rb")).expect("rb");
    assert!(rb.contains("sub, \"title\")"), "{rb}");
    assert!(!rb.contains("sub, \"editor_id\")"), "the request's editor_id is not permitted:\n{rb}");
}
