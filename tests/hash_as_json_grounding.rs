//! `as_json` on a typed Hash or Array is ActiveSupport's core_ext reopen,
//! which no transpiled runtime hosts: on Spinel the request's
//! `params.as_json` (an action logging what it received) and a literal
//! Hash's raised "undefined method 'as_json' for an instance of Hash".
//! It is grounded as `ActiveSupport::JSON.jsonable(...)`, the conversion
//! the runtime already ships: String keys, Symbols as Strings, nested
//! containers converted all the way down.
#[path = "support/emit_and_run.rs"]
mod emit_and_run;

use roundhouse::project::BuildTarget;

const INDEX: &str = "    @articles = Article.includes(:comments).order(created_at: :desc)\n";

fn overlay() -> emit_and_run::Overlay {
    emit_and_run::real_blog()
        .edit(
            "app/controllers/articles_controller.rb",
            INDEX,
            &format!(
                "    if params[:probe] == \"1\"\n      seen = params.as_json\n      echo = {{ kind: :widget, tags: [:a, \"b\"], nested: {{ size: 2 }} }}.as_json\n      render plain: \"#{{seen[\"note\"]}}|#{{echo[\"kind\"]}}|#{{echo[\"tags\"].join(\",\")}}|#{{echo[\"nested\"][\"size\"]}}\"\n      return\n    end\n{INDEX}"
            ),
        )
        .edit(
            "test/controllers/articles_controller_test.rb",
            "  test \"should get new\" do",
            "  test \"as_json on params and a literal hash\" do\n    get articles_url, params: { probe: \"1\", note: \"gadget\" }\n    assert_equal \"gadget|widget|a,b|2\", response.body\n  end\n\n  test \"should get new\" do",
        )
}

#[test]
fn hash_as_json_runs_and_is_grounded_for_spinel() {
    overlay().run_test("test/controllers/articles_controller_test.rb").assert_passes();

    let (emitted, _errors) = overlay().emit(BuildTarget::Spinel);
    let src = std::fs::read_to_string(emitted.join("app/controllers/articles_controller.rb"))
        .expect("emitted articles controller");
    assert!(!src.contains(".as_json"), "a typed as_json survived:\n{src}");
    assert_eq!(src.matches("ActiveSupport::JSON.jsonable(").count(), 2, "{src}");
}
