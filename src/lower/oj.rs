//! Where the `Oj.mimic_JSON` stand-in is allowed to ship.
//!
//! Ingest installs one Ruby class (`ingest::oj`) whose `generate` is
//! `JSON.generate` and whose `load` is the one-argument `JSON.parse`.
//! Ruby, JRuby and Spinel emit that class and run it. Every other
//! transpile target either drops the class (Python, Crystal) or refuses
//! an incidental call inside it (`String#read` on TypeScript, the `JSON`
//! constant on Rust and the other strict targets). Both are the wrong
//! program: a call with nothing behind it, or an error that is not about
//! Oj. This pass is the one gate. It drops the class and rewrites each
//! `Oj.*` call into a `raise`, and records one unsupported diagnostic
//! so a clean check cannot claim the call.

use crate::App;
use crate::dialect::LibraryClassOrigin;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::project::BuildTarget;

/// Targets that emit the stand-in and execute it as Ruby.
pub fn stand_in_runs(target: BuildTarget) -> bool {
    matches!(
        target,
        BuildTarget::Ruby | BuildTarget::Jruby | BuildTarget::Spinel
    )
}

/// Blog, Roda and Futamura keep the app's own call: they run Ruby (the
/// real gem, or residual Rails), not a transpile that would drop the class.
fn keeps_source_call(target: BuildTarget) -> bool {
    matches!(
        target,
        BuildTarget::Blog | BuildTarget::Roda | BuildTarget::Futamura
    )
}

/// `None` when `app` is already what `target` should emit. `Some` is a
/// copy with the stand-in removed and each `Oj.*` call rewritten.
pub fn without_unrun_stand_in(app: &App, target: BuildTarget) -> Option<App> {
    if stand_in_runs(target) || keeps_source_call(target) {
        return None;
    }
    if !app
        .library_classes
        .iter()
        .any(|lc| matches!(lc.origin, Some(LibraryClassOrigin::Oj)))
    {
        return None;
    }
    let mut app = app.clone();
    let target_name = target.as_str();
    crate::lower::for_each_owned_hook_body(&mut app, &mut |owner, body| {
        if owner.is_some_and(|id| id.0.as_str() == "Oj") {
            return;
        }
        rewrite(body, target_name);
    });
    crate::lower::for_each_test_body(&mut app, &mut |body| rewrite(body, target_name));
    for view in &mut app.views {
        rewrite(&mut view.body, target_name);
        if let Some(locals) = &mut view.strict_locals {
            for param in locals {
                if let Some(default) = &mut param.default {
                    rewrite(default, target_name);
                }
            }
        }
    }
    app.library_classes
        .retain(|lc| !matches!(lc.origin, Some(LibraryClassOrigin::Oj)));
    Some(app)
}

fn rewrite(expr: &mut Expr, target: &str) {
    expr.node
        .for_each_child_mut(&mut |child| rewrite(child, target));
    let ExprNode::Send {
        recv: Some(recv),
        method,
        ..
    } = &*expr.node
    else {
        return;
    };
    let on_oj = matches!(&*recv.node, ExprNode::Const { path }
        if path.last().is_some_and(|seg| seg.as_str() == "Oj")
            && path.iter().filter(|seg| !seg.as_str().is_empty()).count() == 1);
    if !on_oj {
        return;
    }
    let method = method.as_str().to_string();
    crate::emit::diagnostics::report_unsupported(
        expr.span,
        target,
        "Oj",
        format!(
            "`Oj.{method}` lowers to the Ruby stand-in, which this target does not emit. \
             The call is refused instead of being written against a missing class"
        ),
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
                        value: "oj stand-in is not emitted on this target".into(),
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
