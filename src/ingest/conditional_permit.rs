// Not left to the params lowering as written: it reads a permit list of literal keys only, so one gated key (`(:mod_note if moderator)`) left the whole list a raw Hash call.
use crate::app::App;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

pub(super) fn apply(app: &mut App) {
    for controller in &mut app.controllers {
        for action in controller.actions_mut() {
            crate::lower::controller_to_library::util::map_expr_mut(&mut action.body, &rewrite);
        }
    }
}

fn gated_key(e: &Expr) -> Option<(Expr, Expr)> {
    let ExprNode::If { cond, then_branch, else_branch } = &*e.node else { return None };
    let is_sym = |x: &Expr| matches!(&*x.node, ExprNode::Lit { value: Literal::Sym { .. } });
    let is_nil = |x: &Expr| matches!(&*x.node, ExprNode::Lit { value: Literal::Nil });
    if is_sym(then_branch) && is_nil(else_branch) {
        return Some((cond.clone(), then_branch.clone()));
    }
    None
}

fn rewrite(e: &Expr) -> Option<Expr> {
    let ExprNode::Send { recv: Some(recv), method, args, block: None, parenthesized } = &*e.node else {
        return None;
    };
    if method.as_str() != "permit" {
        return None;
    }
    let gates: Vec<(Expr, Expr)> = args.iter().filter_map(gated_key).collect();
    if gates.is_empty() {
        return None;
    }
    let keys: Vec<Expr> = args
        .iter()
        .map(|a| gated_key(a).map(|(_, key)| key).unwrap_or_else(|| a.clone()))
        .collect();
    let send = |recv: Expr, method: &str, args: Vec<Expr>| {
        Expr::new(e.span, ExprNode::Send {
            recv: Some(recv),
            method: Symbol::from(method),
            args,
            block: None,
            parenthesized: true,
        })
    };
    let mut out = Expr::new(e.span, ExprNode::Send {
        recv: Some(recv.clone()),
        method: method.clone(),
        args: keys,
        block: None,
        parenthesized: *parenthesized,
    });
    for (cond, key) in gates {
        let dropped = send(out.clone(), "except", vec![key]);
        out = Expr::new(e.span, ExprNode::If { cond, then_branch: out, else_branch: dropped });
    }
    Some(out)
}
