//! Thin adapters over the `ruby-prism` crate surface Roundhouse touches.
//!
//! Prism 2.0.0 renames several of these entry points (`Location::start_offset`
//! → `start`, `Diagnostic::message` → `&[u8]`, `MissingNode` →
//! `ErrorRecoveryNode`). Roundhouse cannot bump yet: `rubydex` still depends
//! on `ruby-prism ^1.9.0`, and both crates `links = "prism"`, so Cargo rejects
//! a mixed graph. These helpers are the single place to flip when that pin
//! moves — call sites that still use the raw 1.9 names should migrate here
//! as they are touched.

use ruby_prism::{Diagnostic, Location, Node};

/// Byte offset where `loc` starts in the parsed source.
#[inline]
pub(crate) fn start_offset(loc: &Location<'_>) -> usize {
    loc.start_offset()
}

/// Byte offset where `loc` ends in the parsed source.
#[inline]
pub(crate) fn end_offset(loc: &Location<'_>) -> usize {
    loc.end_offset()
}

/// `(start, end)` as the `u32` pair [`crate::span::Span`] stores.
#[inline]
pub(crate) fn span_offsets(loc: &Location<'_>) -> (u32, u32) {
    (start_offset(loc) as u32, end_offset(loc) as u32)
}

/// Human-readable text of a Prism diagnostic.
///
/// Prism 2.0 returns `&[u8]` (source encoding; not always UTF-8). Keep the
/// lossy conversion here so the bump does not touch every ingest caller.
#[inline]
pub(crate) fn diagnostic_message(err: &Diagnostic<'_>) -> String {
    err.message().to_string()
}

/// Prism's error-recovery placeholder (`MissingNode` on 1.9;
/// `ErrorRecoveryNode` on 2.0).
#[inline]
pub(crate) fn is_recovery_node(node: &Node<'_>) -> bool {
    node.as_missing_node().is_some()
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
            fn visit_missing_node(&mut self, node: &ruby_prism::MissingNode<'pr>) {
                self.hit = Some(node.as_node());
            }
        }
        let mut finder = Finder { hit: None };
        ruby_prism::Visit::visit(&mut finder, &result.node());
        let node = finder.hit.expect("expected a Prism recovery node for truncated `1 + `");
        assert!(is_recovery_node(&node));
    }

    #[test]
    fn diagnostic_message_is_non_empty() {
        let result = ruby_prism::parse(b"def\n");
        let err = result.errors().next().expect("syntax error");
        assert!(!diagnostic_message(&err).is_empty());
    }
}
