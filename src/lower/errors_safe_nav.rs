//! `record.errors&.full_messages` → `record.errors.full_messages`.
//!
//! `&.` binds a receiver that is not a plain read to a local first
//! (`(__safe_nav1 = record.errors) && __safe_nav1.full_messages`, see
//! `ingest::expr`), and every `errors` lowering matches its receiver as
//! an `errors` send, so the local hid the site from all of them:
//! `full_messages` stayed a call on the Array of full messages. Rails'
//! `errors` is never nil, so the guard never fires, and the send reads
//! `errors` directly. `record&.errors&.m` keeps the guard on `record`:
//! `record && record.errors.m`.
//!
//! Typed: only an `errors` of `ActiveModel::Errors` folds, never an
//! app's own `errors` that may answer nil.

use crate::app::App;
use crate::expr::{BoolOpKind, Expr, ExprNode, LValue};
use crate::ty::Ty;

pub fn apply_errors_safe_nav_lowering(app: &mut App) {
    super::for_each_hook_body(app, &mut rewrite);
    super::for_each_test_body(app, &mut rewrite);
    for view in &mut app.views {
        rewrite(&mut view.body);
    }
}

fn is_model_errors(e: &Expr) -> bool {
    matches!(&*e.node, ExprNode::Send { method, args, block: None, .. } if method.as_str() == "errors" && args.is_empty())
        && matches!(&e.ty, Some(Ty::Class { id, .. }) if id.0.as_str() == "ActiveModel::Errors")
}

fn rewrite(expr: &mut Expr) {
    expr.node.for_each_child_mut(&mut rewrite);
    let ExprNode::BoolOp { op: BoolOpKind::And, left, right, .. } = &mut *expr.node else { return };
    let ExprNode::Assign { target: LValue::Var { name: temp, .. }, value } = &mut *left.node else { return };
    let ExprNode::Send { recv: Some(read), .. } = &*right.node else { return };
    if !matches!(&*read.node, ExprNode::Var { name, .. } if name == temp) {
        return;
    }
    if is_model_errors(value) {
        let value = value.clone();
        let mut send = right.clone();
        let ExprNode::Send { recv, .. } = &mut *send.node else { unreachable!() };
        *recv = Some(value);
        *expr = send;
        return;
    }
    // `record&.errors` is `record && record.errors`; the guard stays on `record`.
    let ExprNode::BoolOp { op: BoolOpKind::And, left: guard, right: errors, .. } = &*value.node else { return };
    if !is_model_errors(errors) {
        return;
    }
    let (guard, errors) = (guard.clone(), errors.clone());
    let mut send = right.clone();
    let ExprNode::Send { recv, .. } = &mut *send.node else { unreachable!() };
    *recv = Some(errors);
    let ExprNode::BoolOp { left, right, .. } = &mut *expr.node else { unreachable!() };
    *left = guard;
    *right = send;
}
