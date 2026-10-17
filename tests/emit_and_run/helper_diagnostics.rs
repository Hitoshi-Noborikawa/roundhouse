//! An `app/helpers` module runs in the view, so its body is checked as a
//! view is, not under the library policy that left it unreported. Two
//! things stay warnings there: an ivar, which is the rendering view's
//! and which the emitted helper reads as Rails does, and nullable
//! primitive arithmetic, which raises where Rails raises.

use super::emit_and_run;
use roundhouse::project::BuildTarget;

fn helper(body: &str) -> emit_and_run::Overlay {
    emit_and_run::real_blog().write("app/helpers/probe_helper.rb", &format!("module ProbeHelper\n{body}end\n"))
}

#[test]
fn a_helper_body_is_checked_as_view_code() {
    let (_emitted, errors) = helper("  def shouted(title)\n    title.to_s.no_such_method_here\n  end\n").emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("no known method `no_such_method_here` on String")), "{errors:#?}");
}

#[test]
fn a_helper_s_ivar_and_nullable_arithmetic_are_not_errors() {
    let (_emitted, errors) = helper(
        "  ORDER = %w[ a b ]\n\n  def page_heading\n    @page_heading_title\n  end\n\n  def next_of(x)\n    ORDER[ORDER.index(x) + 1] || ORDER.first\n  end\n",
    )
    .emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
}

/// Without `sentry-ruby` locked there is no `Sentry`, in Rails or here.
#[test]
fn sentry_is_known_only_with_its_gem() {
    let (_emitted, errors) = helper("  def report(e)\n    Sentry.capture_exception(e)\n  end\n").emit(BuildTarget::Ruby);
    assert!(errors.iter().any(|e| e.contains("capture_exception")), "{errors:#?}");
}
