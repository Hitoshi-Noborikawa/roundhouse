//! Thin adapters over the `ruby-prism` crate surface Roundhouse touches.
//!
//! Prism 2.0.0 renamed Location offsets (`start_offset` → `start` / `end`),
//! made `Diagnostic::message` return `&[u8]`, and replaced `MissingNode`
//! with `ErrorRecoveryNode`. Free functions and [`PrismLocation`] keep call
//! sites stable; new code should prefer the free functions.

use ruby_prism::{Diagnostic, Location, Node};

/// Byte offset where `loc` starts in the parsed source.
#[inline]
pub(crate) fn start_offset(loc: &Location<'_>) -> usize {
    loc.start() as usize
}

/// Byte offset where `loc` ends in the parsed source.
#[inline]
pub(crate) fn end_offset(loc: &Location<'_>) -> usize {
    loc.end() as usize
}

/// `(start, end)` as the `u32` pair [`crate::span::Span`] stores.
#[inline]
pub(crate) fn span_offsets(loc: &Location<'_>) -> (u32, u32) {
    (start_offset(loc) as u32, end_offset(loc) as u32)
}

/// Human-readable text of a Prism diagnostic.
///
/// Prism 2.0 returns `&[u8]` (source encoding; not always UTF-8). Lossy
/// conversion lives here so callers keep a `String`.
#[inline]
pub(crate) fn diagnostic_message(err: &Diagnostic<'_>) -> String {
    String::from_utf8_lossy(err.message()).into_owned()
}

/// Prism's error-recovery placeholder (`ErrorRecoveryNode` on 2.0).
#[inline]
pub(crate) fn is_recovery_node(node: &Node<'_>) -> bool {
    node.as_error_recovery_node().is_some()
}

/// Method-shaped aliases for [`start_offset`] / [`end_offset`] so existing
/// `loc.start_offset()` call sites compile against Prism 2 without a
/// repo-wide rewrite. Bring into scope with `use …::PrismLocation`.
pub(crate) trait PrismLocation {
    fn start_offset(&self) -> usize;
    fn end_offset(&self) -> usize;
}

impl PrismLocation for Location<'_> {
    #[inline]
    fn start_offset(&self) -> usize {
        start_offset(self)
    }

    #[inline]
    fn end_offset(&self) -> usize {
        end_offset(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_match_source_slice() {
        let result = ruby_prism::parse(b"abc");
        let loc = result.node().location();
        assert_eq!(start_offset(&loc), 0);
        assert_eq!(end_offset(&loc), 3);
        assert_eq!(span_offsets(&loc), (0, 3));
        assert_eq!(loc.start_offset(), 0);
        assert_eq!(loc.end_offset(), 3);
        assert_eq!(loc.as_slice(), b"abc");
    }

    #[test]
    fn recovery_node_on_truncated_source() {
        let result = ruby_prism::parse(b"1 + ");
        assert!(result.errors().next().is_some());
        struct Finder<'pr> {
            hit: Option<Node<'pr>>,
        }
        impl<'pr> ruby_prism::Visit<'pr> for Finder<'pr> {
            fn visit_error_recovery_node(
                &mut self,
                node: &ruby_prism::ErrorRecoveryNode<'pr>,
            ) {
                self.hit = Some(node.as_node());
            }
        }
        let mut finder = Finder { hit: None };
        ruby_prism::Visit::visit(&mut finder, &result.node());
        let node = finder
            .hit
            .expect("expected a Prism recovery node for truncated `1 + `");
        assert!(is_recovery_node(&node));
    }

    #[test]
    fn diagnostic_message_is_non_empty() {
        let result = ruby_prism::parse(b"def\n");
        let err = result.errors().next().expect("syntax error");
        assert!(!diagnostic_message(&err).is_empty());
    }
}
