use super::emit_and_run;

/// An initializer's top-level `def` is a method on Object that the emitted
/// app never defines (lobsters' `do_heinous_inline_partial_replacement`,
/// called from a development-only filter); the call is reported, not left
/// to raise NameError.
#[test]
fn a_call_to_an_initializer_top_level_def_is_reported() {
    let app = emit_and_run::real_blog()
        .write("config/initializers/stamp.rb", "def stamp_request\n  Rails.logger.info(\"stamped\")\nend\n")
        .edit(
            "app/controllers/application_controller.rb",
            "  stale_when_importmap_changes\n",
            "  stale_when_importmap_changes\n\n  before_action :stamp\n\n  def stamp\n    stamp_request\n  end\n",
        );
    let (_emitted, errors) = app.emit(roundhouse::project::BuildTarget::Ruby);
    let found: Vec<&String> = errors.iter().filter(|e| e.contains("method an initializer defines at top level not supported")).collect();
    assert_eq!(found.len(), 1, "{errors:#?}");
}
