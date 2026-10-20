//! Refusing a gem's references on a target that does not ship the gem.
//!
//! A gem the Ruby family runs (the real one, or a stand-in class) has
//! nothing behind it on the other targets. Emitting its calls there writes
//! a program against a missing class; leaving them to the target's own
//! walks reports an incidental error that is not about the gem. So each
//! call on the gem's root constant, and each `Root` / `Root::…` constant,
//! becomes a `raise` on every emit body, and each records one unsupported
//! diagnostic, so a clean check cannot claim the reference.

use crate::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

pub(crate) struct Refusal<'a> {
    /// The top-level constant the gem owns (`Oj`, `Faraday`).
    pub root: &'a str,
    /// The diagnostic's construct.
    pub construct: &'a str,
    /// Follows the reference's name in the diagnostic.
    pub why: &'a str,
    /// The message the emitted `raise` carries.
    pub raised: &'a str,
}

/// Whether any emit body names the root constant.
pub(crate) fn mentions(app: &App, root: &str) -> bool {
    let mut found = false;
    crate::lower::for_each_emit_body_ref(app, &mut |body| found |= names_root(body, root));
    found
}

fn names_root(expr: &Expr, root: &str) -> bool {
    if const_name(expr, root).is_some() {
        return true;
    }
    let mut found = false;
    expr.node.for_each_child(&mut |child| found |= names_root(child, root));
    found
}

pub(crate) fn refuse_references(app: &mut App, target: &str, refusal: &Refusal<'_>) {
    crate::lower::for_each_emit_body(app, &mut |body| rewrite(body, target, refusal));
}

fn rewrite(expr: &mut Expr, target: &str, refusal: &Refusal<'_>) {
    if let Some(method) = send_method(expr, refusal.root) {
        rewrite_send_args(expr, target, refusal);
        refuse(expr, target, refusal, &format!("`{}.{method}`", refusal.root));
        return;
    }
    expr.node
        .for_each_child_mut(&mut |child| rewrite(child, target, refusal));
    if let Some(name) = const_name(expr, refusal.root) {
        refuse(expr, target, refusal, &format!("`{name}`"));
    }
}

fn rewrite_send_args(expr: &mut Expr, target: &str, refusal: &Refusal<'_>) {
    let ExprNode::Send { args, block, .. } = &mut *expr.node else {
        return;
    };
    for arg in args {
        rewrite(arg, target, refusal);
    }
    if let Some(block) = block {
        rewrite(block, target, refusal);
    }
}

/// A call whose receiver is the top-level root (`Oj.load`, `::Oj.generate`).
fn send_method(expr: &Expr, root: &str) -> Option<String> {
    let ExprNode::Send {
        recv: Some(recv),
        method,
        ..
    } = &*expr.node
    else {
        return None;
    };
    if const_name(recv, root).as_deref() != Some(root) {
        return None;
    }
    Some(method.as_str().to_string())
}

/// `Oj`, `::Oj`, or `Oj::ParseError`. A constant nested under another class
/// (`Foo::Oj`) is a different name.
fn const_name(expr: &Expr, root: &str) -> Option<String> {
    let ExprNode::Const { path } = &*expr.node else {
        return None;
    };
    let mut parts = path.iter().filter(|seg| !seg.as_str().is_empty());
    if parts.next().is_none_or(|seg| seg.as_str() != root) {
        return None;
    }
    let mut name = String::from(root);
    for seg in parts {
        name.push_str("::");
        name.push_str(seg.as_str());
    }
    Some(name)
}

fn refuse(expr: &mut Expr, target: &str, refusal: &Refusal<'_>, name: &str) {
    crate::emit::diagnostics::report_unsupported(
        expr.span,
        target,
        refusal.construct,
        format!("{name} {}", refusal.why),
    );
    let span = expr.span;
    let ty = expr.ty.clone();
    let mut raised = Expr::new(
        span,
        ExprNode::Send {
            recv: None,
            method: Symbol::from("raise"),
            args: vec![Expr::new(
                span,
                ExprNode::Lit {
                    value: Literal::Str {
                        value: refusal.raised.into(),
                    },
                },
            )],
            block: None,
            parenthesized: true,
        },
    );
    raised.ty = ty;
    *expr = raised;
}
