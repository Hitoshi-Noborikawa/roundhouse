use std::collections::BTreeSet;

use crate::App;
use crate::build_target::BuildTarget;
use crate::expr::{Arm, Expr, ExprNode, Literal, Pattern};
use crate::ident::Symbol;
use crate::span::Span;

fn case_runs(target: BuildTarget) -> bool {
    matches!(target, BuildTarget::Ruby | BuildTarget::Jruby)
}

fn keeps_source_call(target: BuildTarget) -> bool {
    matches!(target, BuildTarget::Blog | BuildTarget::Roda | BuildTarget::Futamura)
}

pub fn lower_for_target(app: &App, target: BuildTarget) -> Option<App> {
    if keeps_source_call(target) {
        return None;
    }
    let mut found = false;
    crate::lower::for_each_emit_body_ref(app, &mut |body| found |= has_call(body));
    if !found {
        return None;
    }
    let names = app_constants(app);
    let mut app = app.clone();
    let target_name = target.as_str();
    let runs = case_runs(target);
    crate::lower::for_each_emit_body(&mut app, &mut |body| rewrite(body, &names, runs, target_name));
    Some(app)
}

// Only the app's own constants: a name from run-time data must not reach every class in the process
fn app_constants(app: &App) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    names.extend(app.models.iter().map(|m| m.name.0.as_str().to_string()));
    names.extend(app.controllers.iter().map(|c| c.name.0.as_str().to_string()));
    names.extend(
        app.library_classes
            .iter()
            .filter(|lc| lc.origin.is_none())
            .map(|lc| lc.name.0.as_str().to_string()),
    );
    names
}

fn constantize_method(expr: &Expr) -> Option<&'static str> {
    let ExprNode::Send { recv: Some(_), method, args, block: None, .. } = &*expr.node else { return None };
    if !args.is_empty() {
        return None;
    }
    match method.as_str() {
        "constantize" => Some("constantize"),
        "safe_constantize" => Some("safe_constantize"),
        _ => None,
    }
}

// `lower::record_snapshot` rewrites `name.constantize.instantiate(attrs)` itself, so it is left alone here
fn is_snapshot_rebuild(expr: &Expr) -> bool {
    let ExprNode::Send { recv: Some(recv), method, args, .. } = &*expr.node else { return false };
    method.as_str() == "instantiate" && args.len() == 1 && constantize_method(recv) == Some("constantize")
}

fn has_call(expr: &Expr) -> bool {
    if is_snapshot_rebuild(expr) {
        let ExprNode::Send { args, .. } = &*expr.node else { return false };
        return args.iter().any(has_call);
    }
    let mut found = constantize_method(expr).is_some();
    expr.node.for_each_child(&mut |child| found |= has_call(child));
    found
}

fn rewrite(expr: &mut Expr, names: &BTreeSet<String>, runs: bool, target: &str) {
    if is_snapshot_rebuild(expr) {
        if let ExprNode::Send { args, .. } = &mut *expr.node {
            for arg in args {
                rewrite(arg, names, runs, target);
            }
        }
        return;
    }
    expr.node.for_each_child_mut(&mut |child| rewrite(child, names, runs, target));
    let Some(method) = constantize_method(expr) else { return };
    let span = expr.span;
    let ExprNode::Send { recv: Some(recv), .. } = &*expr.node else { return };
    let recv = recv.clone();
    *expr = if runs {
        closed_case(span, recv, names, method == "safe_constantize")
    } else {
        let text = crate::emit::diagnostics::report_unsupported(
            span,
            target,
            method,
            format!(
                "`String#{method}` resolves a name only over the app's own constants, \
                 as a `case` that this target does not run"
            ),
        );
        refusal(span, &text)
    };
}

fn closed_case(span: Span, recv: Expr, names: &BTreeSet<String>, safe: bool) -> Expr {
    let mut arms: Vec<Arm> = names
        .iter()
        .map(|name| Arm {
            pattern: Pattern::Lit { value: Literal::Str { value: name.clone() } },
            guard: None,
            body: Expr::new(span, ExprNode::Const { path: name.split("::").map(Symbol::from).collect() }),
        })
        .collect();
    let fallback = if safe {
        crate::lower::typing::nil_lit()
    } else {
        let message = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: "uninitialized constant".into() } });
        let error = Expr::new(
            span,
            ExprNode::Send {
                recv: Some(Expr::new(span, ExprNode::Const { path: vec![Symbol::from("NameError")] })),
                method: Symbol::from("new"),
                args: vec![message],
                block: None,
                parenthesized: true,
            },
        );
        Expr::new(span, ExprNode::Raise { value: error })
    };
    arms.push(Arm { pattern: Pattern::Wildcard, guard: None, body: fallback });
    Expr::new(span, ExprNode::Case { scrutinee: recv, arms })
}

fn refusal(span: Span, text: &str) -> Expr {
    let error = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(Expr::new(span, ExprNode::Const { path: vec![Symbol::from("NotImplementedError")] })),
            method: Symbol::from("new"),
            args: vec![Expr::new(span, ExprNode::Lit { value: Literal::Str { value: text.to_string() } })],
            block: None,
            parenthesized: true,
        },
    );
    Expr::new(span, ExprNode::Raise { value: error })
}
