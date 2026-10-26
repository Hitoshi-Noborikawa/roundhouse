//! A `where` value written as a Range of non-numeric ends
//! (`where(created_at: 7.days.ago..)`) becomes
//! `ActiveRecord::Bounds.new(lower, upper, exclusive)` on the ruby family.
//!
//! The runtime's `column_predicate` reads a Bounds as it reads a Range (the
//! same `>=` / `<` / `<=` SQL). Spinel keeps a Range's bounds in integer
//! slots, so a Range of Times cannot be built there; CRuby takes the same
//! rewrite, so the two stay one program.

use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::ty::Ty;

const CONDITION_METHODS: &[&str] = &["where", "not", "rewhere", "or", "find_by", "find_by!", "exists?", "find_or_create_by", "find_or_initialize_by"];

pub fn lower_for_target(app: &crate::App, target: crate::build_target::BuildTarget) -> Option<crate::App> {
    use crate::build_target::BuildTarget;
    if !matches!(target, BuildTarget::Spinel | BuildTarget::Ruby | BuildTarget::Jruby) {
        return None;
    }
    let mut app = app.clone();
    crate::lower::for_each_emit_body(&mut app, &mut rewrite);
    Some(app)
}

fn rewrite(e: &mut Expr) {
    e.node.for_each_child_mut(&mut rewrite);
    let ExprNode::Send { method, args, .. } = &mut *e.node else { return };
    if !CONDITION_METHODS.contains(&method.as_str()) {
        return;
    }
    for arg in args.iter_mut() {
        bounds_in_conditions(arg);
    }
}

/// Each Range value of a conditions Hash, nested ones (`from_comment: { created_at: … }`) included.
fn bounds_in_conditions(e: &mut Expr) {
    let ExprNode::Hash { entries, .. } = &mut *e.node else { return };
    for (_, value) in entries.iter_mut() {
        if matches!(&*value.node, ExprNode::Hash { .. }) {
            bounds_in_conditions(value);
        } else if let Some(bounds) = as_bounds(value) {
            *value = bounds;
        } else if let ExprNode::Array { elements, .. } = &mut *value.node {
            // `where(created_at: [nil, 7.days.ago..])`: each Range element is a bound of its own.
            for element in elements.iter_mut() {
                if let Some(bounds) = as_bounds(element) {
                    *element = bounds;
                }
            }
        }
    }
}

fn as_bounds(value: &Expr) -> Option<Expr> {
    let ExprNode::Range { begin, end, exclusive } = &*value.node else { return None };
    let numeric = |end: &Option<Expr>| match end {
        None => true,
        Some(e) => matches!(e.ty.as_ref(), Some(Ty::Int | Ty::Float | Ty::Nil)),
    };
    if numeric(begin) && numeric(end) {
        return None;
    }
    let span = value.span;
    let nil = || Expr { ty: Some(Ty::Nil), ..Expr::new(span, ExprNode::Lit { value: Literal::Nil }) };
    let bounds_class = Expr::new(span, ExprNode::Const { path: vec![Symbol::from("ActiveRecord"), Symbol::from("Bounds")] });
    let call = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(bounds_class),
            method: Symbol::from("new"),
            args: vec![
                begin.clone().unwrap_or_else(nil),
                end.clone().unwrap_or_else(nil),
                Expr { ty: Some(Ty::Bool), ..Expr::new(span, ExprNode::Lit { value: Literal::Bool { value: *exclusive } }) },
            ],
            block: None,
            parenthesized: true,
        },
    );
    Some(Expr {
        ty: Some(Ty::Class { id: crate::ident::ClassId(Symbol::from("ActiveRecord::Bounds")), args: vec![].into() }),
        ..call
    })
}
