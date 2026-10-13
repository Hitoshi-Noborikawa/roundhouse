//! A model-body `include` guarded by `begin … rescue NameError … end`.
//!
//! ```ruby
//! class Part < ApplicationRecord
//!   begin
//!     include(Chime)
//!   rescue NameError
//!   end
//! end
//! ```
//!
//! "Include it when the module is there." The body walk keeps the
//! `begin` as one unknown item, and nothing after it reads includes out
//! of a `begin`: the module never reached the class. This pass runs
//! once every file is ingested, so it knows which modules the app
//! defines, and answers what Ruby does: the includes run in order, and
//! the first one naming a module the app does not define raises the
//! `NameError` the rescue swallows. The block is the includes before
//! that one (all of them when every module is defined).
//!
//! Only this exact shape is read: a body of bare `include Const` calls,
//! a rescue clause that catches `NameError` (naming it, `StandardError`
//! or `Exception`, or a bare `rescue`), every rescue body empty (or
//! `nil`), no `else` and no `ensure`. Any other `begin` is left as it
//! was: a rescue that only names `LoadError` does not catch the
//! `NameError`, and a rescue with a body (a fallback include) does
//! something this pass does not model.

use std::collections::HashSet;

use crate::app::App;
use crate::dialect::ModelBodyItem;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

pub(super) fn splice_guarded_includes(app: &mut App) {
    let known: HashSet<ClassId> = app
        .library_classes
        .iter()
        .map(|lc| lc.name.clone())
        .chain(app.concern_model_items.keys().cloned())
        .collect();
    for model in &mut app.models {
        let owner = model.name.0.as_str().to_string();
        let mut body = Vec::with_capacity(model.body.len());
        for item in std::mem::take(&mut model.body) {
            let ModelBodyItem::Unknown { expr, leading_comments, leading_blank_line } = &item else {
                body.push(item);
                continue;
            };
            let Some(includes) = guarded_includes(expr) else {
                body.push(item);
                continue;
            };
            // `include A, B` looks both up before including either, so one
            // undefined name stops the whole call.
            let includes: Vec<&Expr> = includes
                .into_iter()
                .take_while(|include| include_consts(include).all(|path| resolves(&owner, path, &known)))
                .collect();
            let mut comments = leading_comments.clone();
            let mut blank = *leading_blank_line;
            for include in includes {
                body.push(ModelBodyItem::Unknown {
                    expr: include.clone(),
                    leading_comments: std::mem::take(&mut comments),
                    leading_blank_line: std::mem::take(&mut blank),
                });
            }
        }
        model.body = body;
    }
}

/// The `include` calls of a guarded block, or None when `expr` is not
/// one.
fn guarded_includes(expr: &Expr) -> Option<Vec<&Expr>> {
    let ExprNode::BeginRescue { body, rescues, else_branch: None, ensure: None, implicit: false } =
        &*expr.node
    else {
        return None;
    };
    // `NameError` or an ancestor; a bare `rescue` is `StandardError`.
    let catches_name_error = rescues.iter().any(|clause| {
        clause.classes.is_empty()
            || clause.classes.iter().any(|class| {
                matches!(&*class.node, ExprNode::Const { path }
                    if path.last().is_some_and(|n| matches!(n.as_str(), "NameError" | "StandardError" | "Exception"))
                        && (path.len() == 1 || (path.len() == 2 && path[0].as_str().is_empty())))
            })
    });
    let rescues_do_nothing = rescues.iter().all(|clause| is_empty_body(&clause.body));
    if !catches_name_error || !rescues_do_nothing {
        return None;
    }
    let statements: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    let is_include = |e: &&Expr| {
        matches!(&*e.node, ExprNode::Send { recv: None, method, args, block: None, .. }
            if method.as_str() == "include"
                && !args.is_empty()
                && args.iter().all(|a| matches!(&*a.node, ExprNode::Const { .. })))
    };
    (!statements.is_empty() && statements.iter().all(is_include)).then_some(statements)
}

/// A rescue body that does nothing: empty, or a lone `nil`.
fn is_empty_body(body: &Expr) -> bool {
    match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().all(is_empty_body),
        ExprNode::Lit { value: crate::expr::Literal::Nil } => true,
        _ => false,
    }
}

fn include_consts(include: &Expr) -> impl Iterator<Item = &[Symbol]> {
    let args: &[Expr] = match &*include.node {
        ExprNode::Send { args, .. } => args,
        _ => &[],
    };
    args.iter().filter_map(|a| match &*a.node {
        ExprNode::Const { path } => Some(path.as_slice()),
        _ => None,
    })
}

/// `path` as written inside `owner` names a module the app defines:
/// itself, or under one of the owner's nesting scopes (innermost
/// first), as Ruby's lexical lookup would find it.
fn resolves(owner: &str, path: &[Symbol], known: &HashSet<ClassId>) -> bool {
    let tail = path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("::");
    let scope: Vec<&str> = owner.split("::").collect();
    (0..=scope.len()).rev().any(|depth| {
        let candidate = if depth == 0 {
            tail.clone()
        } else {
            format!("{}::{tail}", scope[..depth].join("::"))
        };
        known.contains(&ClassId(Symbol::from(candidate.as_str())))
    })
}
