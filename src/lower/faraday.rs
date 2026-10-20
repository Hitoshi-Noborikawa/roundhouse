//! Where Faraday is allowed to ship.
//!
//! The analyzer types Faraday from the gem catalog (`catalog::gems`), and
//! the Ruby and JRuby trees declare and require the real gem
//! (`gems::RUNTIME_GEMS`), so the app's own calls run there. No other
//! transpile target has an HTTP client behind the constant: each
//! reference is refused there, a `raise` with one unsupported diagnostic.

use crate::App;
use crate::build_target::BuildTarget;

/// Targets that run the real gem: Ruby and JRuby load it, and Blog, Roda
/// and Futamura keep the app's own Ruby.
fn ships_gem(target: BuildTarget) -> bool {
    matches!(
        target,
        BuildTarget::Ruby | BuildTarget::Jruby | BuildTarget::Blog | BuildTarget::Roda | BuildTarget::Futamura
    )
}

/// `None` when `app` is already what `target` should emit. `Some` is a
/// copy with each `Faraday` call or constant rewritten into a refusal.
pub fn without_unshipped_gem(app: &App, target: BuildTarget) -> Option<App> {
    if ships_gem(target) || !crate::lower::gem_refusal::mentions(app, "Faraday") {
        return None;
    }
    let mut app = app.clone();
    crate::lower::gem_refusal::refuse_references(
        &mut app,
        target.as_str(),
        &crate::lower::gem_refusal::Refusal {
            root: "Faraday",
            construct: "Faraday",
            why: "is the faraday gem, which only the ruby and jruby trees ship. \
                  The reference is refused instead of being written against a missing HTTP client",
            raised: "faraday is not shipped on this target",
        },
    );
    Some(app)
}
