use roundhouse::App;
use roundhouse::ingest::ingest_library_classes;

const SOURCE: &str = r#"class Presenter
  attr_reader :label, :scope, :now

  def initialize(label:, scope: nil, now: Time.now)
    @label = label
    @scope = scope
    @now = now
  end

  def summary
    @label
  end
end

class PresenterCaller
  def self.build
    Presenter.new(label: "first", scope: nil)
  end

  def self.replace_summary
    Presenter.define_method(:summary) { "replacement" }
  end
end
"#;

#[test]
fn redefining_an_unrelated_method_keeps_constructor_keywords_supported() {
    let mut app = App::new();
    app.library_classes
        .extend(ingest_library_classes(SOURCE.as_bytes(), "presenter.rb").expect("ingest"));
    let diagnostics = roundhouse::session::analyze_and_lower(&mut app);
    assert!(
        !diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            roundhouse::diagnostic::DiagnosticKind::Unsupported { construct, .. }
                if construct.as_str() == "constructor keyword arguments"
        )),
        "{diagnostics:#?}"
    );
    let emitted = roundhouse::emit::ruby::emit_library(&app)
        .into_iter()
        .map(|file| file.content)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !emitted.contains("constructor keyword arguments not supported"),
        "{emitted}"
    );
}

#[test]
fn named_mutations_only_refuse_constructor_methods_or_unknown_names() {
    for (mutation, refused) in [
        ("define_method(:summary) { 'changed' }", false),
        ("define_singleton_method('summary') { 'changed' }", false),
        ("remove_method(:summary)", false),
        ("undef_method(:summary)", false),
        ("define_method(:initialize) { |**options| }", true),
        (
            "define_singleton_method(:new) { |**options| options }",
            true,
        ),
        ("remove_method(:summary, :initialize)", true),
        ("undef_method(:initialize)", true),
        ("define_method(method_name) { 'changed' }", true),
        ("remove_method(method_name)", true),
    ] {
        let source = SOURCE.replace("define_method(:summary) { \"replacement\" }", mutation);
        let mut app = App::new();
        app.library_classes
            .extend(ingest_library_classes(source.as_bytes(), "mutation.rb").expect("ingest"));
        let diagnostics = roundhouse::session::analyze_and_lower(&mut app);
        let actual = diagnostics.iter().any(|diagnostic| {
            matches!(
                &diagnostic.kind,
                roundhouse::diagnostic::DiagnosticKind::Unsupported { construct, .. }
                    if construct.as_str() == "constructor keyword arguments"
            )
        });
        assert_eq!(actual, refused, "{mutation}: {diagnostics:#?}");
    }
}

#[test]
fn a_method_added_hook_still_makes_constructor_lookup_unknown() {
    let source = SOURCE.replace(
        "  def summary\n",
        "  def self.method_added(name)\n  end\n\n  def summary\n",
    );
    let mut app = App::new();
    app.library_classes
        .extend(ingest_library_classes(source.as_bytes(), "hook.rb").expect("ingest"));
    let diagnostics = roundhouse::session::analyze_and_lower(&mut app);
    assert!(
        diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            roundhouse::diagnostic::DiagnosticKind::Unsupported { construct, .. }
                if construct.as_str() == "constructor keyword arguments"
        )),
        "{diagnostics:#?}"
    );
}
