//! A route helper a view or controller calls without the segments its route requires
//! recalls them from the current request, as Rails' `url_for` does:
//! on lobsters' `/~bob`, `user_ban_path` is `/~bob/ban`. The call takes
//! `path_parameters.fetch("username")` for each segment, and a view
//! reading `path_parameters` is threaded the request's own
//! (`view_to_library::request_context_views`).

use crate::app::App;
use crate::det::HashMap;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;
use crate::span::Span;
use crate::ty::Ty;

pub fn apply_route_param_recall(app: &mut App) {
    let required: HashMap<String, Vec<Symbol>> = super::lower_routes_to_library_functions(app)
        .into_iter()
        .filter_map(|f| {
            let names: Vec<Symbol> = f
                .params
                .iter()
                .filter(|p| p.default.is_none() && !p.keyword && !p.rest)
                .map(|p| p.name.clone())
                .collect();
            (!names.is_empty()).then(|| (f.name.as_str().to_string(), names))
        })
        .collect();
    if required.is_empty() {
        return;
    }
    for controller in &mut app.controllers {
        for action in controller.actions_mut() {
            rewrite(&mut action.body, &required);
        }
    }
    for view in &mut app.views {
        // Not a jbuilder view: its render tree threads no request context.
        if view.jbuilder {
            continue;
        }
        rewrite(&mut view.body, &required);
    }
}

fn rewrite(expr: &mut Expr, required: &HashMap<String, Vec<Symbol>>) {
    expr.node.for_each_child_mut(&mut |c| rewrite(c, required));
    let ExprNode::Send { recv: None, method, args, block: None, parenthesized } = &mut *expr.node else {
        return;
    };
    // Not a call passing some of the segments: which ones Rails recalls then depends on the request's own, known only at run time.
    if args.iter().any(|a| !matches!(&*a.node, ExprNode::Hash { kwargs: true, .. })) {
        return;
    }
    let m = method.as_str();
    let path = match m.strip_suffix("_url") {
        Some(stem) => format!("{stem}_path"),
        None => m.to_string(),
    };
    let Some(names) = required.get(&path) else { return };
    // Not when the options name a segment (`mod_mail_path(id: …)`): the path lowering binds those by name already.
    let names_a_segment = args.iter().any(|a| match &*a.node {
        ExprNode::Hash { entries, .. } => entries.iter().any(|(k, _)| match &*k.node {
            ExprNode::Lit { value: Literal::Sym { value } } => names.contains(value),
            _ => true,
        }),
        _ => false,
    });
    if names_a_segment {
        return;
    }
    let span = expr.span;
    let options = std::mem::take(args);
    *args = names.iter().map(|n| recalled(n, span)).chain(options).collect();
    *parenthesized = true;
}

fn recalled(name: &Symbol, span: Span) -> Expr {
    let mut params = Expr::new(
        span,
        ExprNode::Send {
            recv: None,
            method: Symbol::from("path_parameters"),
            args: Vec::new(),
            block: None,
            parenthesized: false,
        },
    );
    params.ty = Some(Ty::Hash { key: Ty::Str.into(), value: Ty::Str.into() });
    let mut key = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: name.as_str().to_string() } });
    key.ty = Some(Ty::Str);
    // Not left untyped: the helper's segment types come from what its calls pass, and an untyped one leaves an Integer slot a String reaches.
    let mut fetched = Expr::new(
        span,
        ExprNode::Send {
            recv: Some(params),
            method: Symbol::from("fetch"),
            args: vec![key],
            block: None,
            parenthesized: true,
        },
    );
    fetched.ty = Some(Ty::Str);
    fetched
}
