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
//!
//! A constant that names an app **class** is not spliced: Ruby raises
//! `TypeError` (rescued when the clause catches `StandardError`). A
//! Ruby core/stdlib module (`Enumerable`, …) counts as defined even
//! though it is outside the app set. Relative lookup is root-only —
//! `class A::B` does not open `A` lexically, so owner-name splitting
//! would invent a false scope.

use std::collections::HashSet;

use crate::app::App;
use crate::dialect::ModelBodyItem;
use crate::expr::{Expr, ExprNode};
use crate::ident::{ClassId, Symbol};

pub(super) fn splice_guarded_includes(app: &mut App) {
    let known_modules: HashSet<ClassId> = app
        .library_classes
        .iter()
        .filter(|lc| lc.is_module)
        .map(|lc| lc.name.clone())
        .chain(app.concern_model_items.keys().cloned())
        .collect();
    let known_classes: HashSet<ClassId> = app
        .library_classes
        .iter()
        .filter(|lc| !lc.is_module)
        .map(|lc| lc.name.clone())
        .chain(app.models.iter().map(|m| m.name.clone()))
        .collect();
    for model in &mut app.models {
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
            // A known class target would TypeError under `include`; keep
            // the rescue when the clause can catch that.
            if includes.iter().any(|include| {
                include_consts(include).any(|path| matches!(lookup(path, &known_modules, &known_classes), Lookup::Class))
            }) {
                body.push(item);
                continue;
            }
            // `include A, B` looks both up before including either, so one
            // undefined name stops the whole call.
            let includes: Vec<&Expr> = includes
                .into_iter()
                .take_while(|include| {
                    include_consts(include).all(|path| {
                        matches!(lookup(path, &known_modules, &known_classes), Lookup::Module)
                    })
                })
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lookup {
    /// An includable module the app (or Ruby core) defines.
    Module,
    /// An app class — `include` would TypeError.
    Class,
    /// Not in the app set and not a known core module.
    Unknown,
}

/// Root-only const lookup. Absolute paths strip a leading empty segment;
/// relative paths do **not** invent scopes from a qualified class name
/// (`class A::B` does not open `A` lexically).
fn lookup(path: &[Symbol], modules: &HashSet<ClassId>, classes: &HashSet<ClassId>) -> Lookup {
    let absolute = path.first().is_some_and(|s| s.as_str().is_empty());
    let segments: Vec<&str> = if absolute {
        path.iter().skip(1).map(|s| s.as_str()).collect()
    } else {
        path.iter().map(|s| s.as_str()).collect()
    };
    if segments.is_empty() {
        return Lookup::Unknown;
    }
    let name = segments.join("::");
    let id = ClassId(Symbol::from(name.as_str()));
    if modules.contains(&id) || is_ruby_core_module(&segments) {
        Lookup::Module
    } else if classes.contains(&id) {
        Lookup::Class
    } else {
        Lookup::Unknown
    }
}

/// Core/stdlib modules a guarded `include` may name without the app
/// defining them. Kept small — only names that are modules in Ruby and
/// appear in real Rails apps' optional-include guards.
fn is_ruby_core_module(segments: &[&str]) -> bool {
    matches!(
        segments,
        ["Enumerable"]
            | ["Comparable"]
            | ["Kernel"]
            | ["Forwardable"]
            | ["SingleForwardable"]
            | ["JSON"]
            | ["FileUtils"]
    )
}
