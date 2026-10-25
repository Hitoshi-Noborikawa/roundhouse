//! `normalizes :email_address, with: ->(e) { e.strip.downcase }` (Rails
//! 7.1) — the declaration the Rails 8 authentication generator puts on
//! every `User`.
//!
//! Rails applies the normalization in two places, and both matter for a
//! sign-in form:
//!
//!   - on assignment: `User.new(email_address: " A@B.C ").email_address`
//!     is `"a@b.c"`, and so is what gets saved;
//!   - to the matching keyword of a finder: `User.find_by(email_address:
//!     " A@B.C ")` finds that row, which is what lets a user sign in with
//!     the capitalization they typed.
//!
//! The lambda becomes one synthesized class method,
//! `Model._normalize_<attr>(value)`, with the lambda's body inlined over
//! `value` and nil passed through (Rails' `apply_to_nil: false`
//! default). The column writer calls it, and the finder rewrite below
//! wraps the keyword's value in it — one copy of the body, and the
//! value expression evaluated once.
//!
//! DIVERGENCE: hydration assigns through the same column writer, so a
//! row loaded from the database is normalized too, where Rails leaves an
//! existing row's value alone until it is reassigned. The two agree for
//! every row written by the app after the declaration.
//!
//! `apply_to_nil: true` drops the nil pass-through: an assigned nil is
//! normalized too (`->(t) { t.to_s }` makes it ""). `normalize_value_for
//! (:attr, value)` on the model (a class method, `self.class.` in an
//! instance method, a scope) is the same call as the column writer's.
//!
//! Not reproduced, and left alone rather than half-applied: a `with:`
//! that is not a one-parameter lambda literal.

use std::collections::HashMap;

use indexmap::IndexMap;

use crate::app::App;
use crate::dialect::{AccessorKind, MethodDef, MethodReceiver, Model, ModelBodyItem, Param};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol, VarId};
use crate::span::Span;
use crate::ty::Ty;

/// The normalizing class method's name for `attr`.
pub(crate) fn normalizer_name(attr: &Symbol) -> Symbol {
    Symbol::from(format!("_normalize_{}", attr.as_str()))
}

/// Every attribute `model` declares a reproducible normalization for,
/// with the lambda's parameter and body, in declaration order (an
/// `IndexMap`: a repeated `normalizes :attr` keeps its first position
/// but the later declaration's body wins, same as a Ruby `Hash`).
pub(crate) fn normalizations(model: &Model) -> IndexMap<Symbol, (Symbol, Expr, bool)> {
    let mut out = IndexMap::new();
    for item in &model.body {
        let ModelBodyItem::Unknown { expr, .. } = item else { continue };
        let ExprNode::Send { recv: None, method, args, .. } = &*expr.node else { continue };
        if method.as_str() != "normalizes" {
            continue;
        }
        let mut attrs = Vec::new();
        let mut with = None;
        let mut apply_to_nil = false;
        let mut supported = true;
        for arg in args {
            match &*arg.node {
                ExprNode::Lit { value: Literal::Sym { value } } => attrs.push(value.clone()),
                ExprNode::Hash { entries, .. } => {
                    for (k, v) in entries {
                        let ExprNode::Lit { value: Literal::Sym { value: key } } = &*k.node else {
                            supported = false;
                            continue;
                        };
                        match (key.as_str(), &*v.node) {
                            ("with", ExprNode::Lambda { params, rest_param: None, extra_params, body, .. })
                                if params.len() == 1 && extra_params.is_empty() =>
                            {
                                with = Some((params[0].clone(), body.clone()));
                            }
                            ("apply_to_nil", ExprNode::Lit { value: Literal::Bool { value } }) => apply_to_nil = *value,
                            _ => supported = false,
                        }
                    }
                }
                _ => supported = false,
            }
        }
        let (Some((param, body)), true) = (with, supported) else { continue };
        for attr in attrs {
            out.insert(attr, (param.clone(), body.clone(), apply_to_nil));
        }
    }
    out
}

/// `Model._normalize_<attr>(value)` for each normalized attribute that
/// is one of the model's columns: `value.nil? ? nil : <body>`, or the
/// body alone under `apply_to_nil: true`.
pub(crate) fn push_normalize_methods(methods: &mut Vec<MethodDef>, model: &Model, columns: &HashMap<Symbol, Ty>) {
    for (attr, (param, body, apply_to_nil)) in normalizations(model) {
        let Some(col_ty) = columns.get(&attr) else { continue };
        let name = normalizer_name(&attr);
        if methods.iter().any(|m| m.receiver == MethodReceiver::Class && m.name == name) {
            continue;
        }
        let value = Symbol::from("value");
        let read = || synthetic(ExprNode::Var { id: VarId(0), name: value.clone() });
        let mut normalized = body.clone();
        crate::lower::case_lambda::subst(&mut normalized, &param, &read());
        let nil_check = synthetic(ExprNode::Send {
            recv: Some(read()),
            method: Symbol::from("nil?"),
            args: vec![],
            block: None,
            parenthesized: false,
        });
        let body = if apply_to_nil {
            normalized
        } else {
            synthetic(ExprNode::If {
                cond: nil_check,
                then_branch: synthetic(ExprNode::Lit { value: Literal::Nil }),
                else_branch: normalized,
            })
        };
        methods.push(MethodDef {
            visibility: crate::dialect::MethodVisibility::Public,
            unsupported_formals: None,
            has_anonymous_block: false,
            name_span: Span::synthetic(),
            name,
            receiver: MethodReceiver::Class,
            params: vec![Param::positional(value.clone())],
            body,
            signature: Some(super::model_to_library::fn_sig(vec![(value, col_ty.clone())], col_ty.clone())),
            effects: crate::effect::EffectSet::default(),
            enclosing_class: Some(model.name.0.clone()),
            kind: AccessorKind::Method,
            is_async: false,
            mutates_self: false,
            block_param: None,
        });
    }
}

/// The column writer's right-hand side for a normalized attribute:
/// `Model._normalize_<attr>(value)`.
pub(crate) fn normalized_write(model: &Model, attr: &Symbol, value: Expr) -> Option<Expr> {
    if !normalizations(model).contains_key(attr) {
        return None;
    }
    Some(call_normalizer(&model.name, attr, value))
}

fn call_normalizer(model: &ClassId, attr: &Symbol, value: Expr) -> Expr {
    // The model's constant, a synthesized class reference: no source span
    // (the value's would make the resolver look for a constant there).
    let mut model_ref = Expr::new(
        Span::synthetic(),
        ExprNode::Const { path: model.0.as_str().split("::").map(Symbol::from).collect() },
    );
    model_ref.decisions |= crate::expr::RESOLVED_CLASS_REF;
    let mut e = Expr::new(
        value.span,
        ExprNode::Send {
            recv: Some(model_ref),
            method: normalizer_name(attr),
            args: vec![value.clone()],
            block: None,
            parenthesized: true,
        },
    );
    e.ty = value.ty.clone();
    e
}

/// Finder keywords: `Model.find_by(email_address: v)` (and `find_by!`,
/// `where`, `exists?`, `find_or_create_by`, `find_or_initialize_by`, on
/// the class or a relation over it) normalize `v` the way assignment
/// would. Runs after every pass that synthesizes a `find_by`
/// (`authenticate_by` expands into one).
pub fn apply_normalizes_finder_lowering(app: &mut App) {
    let normalized: HashMap<ClassId, Vec<Symbol>> = app
        .models
        .iter()
        .filter_map(|m| {
            let attrs: Vec<Symbol> = normalizations(m).into_keys().collect();
            (!attrs.is_empty()).then(|| (m.name.clone(), attrs))
        })
        .collect();
    if normalized.is_empty() {
        return;
    }
    super::for_each_owned_hook_body(app, &mut |owner, body| rewrite(body, &normalized, owner));
}

const FINDERS: &[&str] =
    &["find_by", "find_by!", "where", "exists?", "find_or_create_by", "find_or_initialize_by", "create_or_find_by", "create_or_find_by!"];

fn rewrite(e: &mut Expr, normalized: &HashMap<ClassId, Vec<Symbol>>, owner: Option<&ClassId>) {
    e.node.for_each_child_mut(&mut |c| rewrite(c, normalized, owner));
    if let Some(call) = normalize_value_for(e, normalized, owner) {
        *e = call;
        return;
    }
    let ExprNode::Send { recv: Some(r), method, args, .. } = &mut *e.node else { return };
    if !FINDERS.contains(&method.as_str()) {
        return;
    }
    let class = match r.ty.as_ref() {
        Some(Ty::Class { id, .. }) => id.clone(),
        Some(Ty::Relation { of }) => of.clone(),
        _ => match &*r.node {
            ExprNode::Const { path } => {
                ClassId(Symbol::from(path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("::")))
            }
            _ => return,
        },
    };
    let Some(attrs) = normalized.get(&class) else { return };
    for arg in args.iter_mut() {
        let ExprNode::Hash { entries, .. } = &mut *arg.node else { continue };
        for (k, v) in entries.iter_mut() {
            let ExprNode::Lit { value: Literal::Sym { value: key } } = &*k.node else { continue };
            if !attrs.contains(key) || is_normalizer_call(v) {
                continue;
            }
            *v = call_normalizer(&class, key, v.clone());
        }
    }
}

/// `normalize_value_for(:attr, value)` on a model that normalizes
/// `attr` — receiver-less or `self.` in its own class methods and
/// scopes, `self.class.` in its instance methods, or the model's
/// constant — as `Model._normalize_<attr>(value)`. Another attribute is
/// left alone (Rails casts it by its type there).
fn normalize_value_for(e: &Expr, normalized: &HashMap<ClassId, Vec<Symbol>>, owner: Option<&ClassId>) -> Option<Expr> {
    let ExprNode::Send { recv, method, args, .. } = &*e.node else { return None };
    if method.as_str() != "normalize_value_for" || args.len() != 2 {
        return None;
    }
    let ExprNode::Lit { value: Literal::Sym { value: attr } } = &*args[0].node else { return None };
    let class = match recv.as_ref().map(|r| &*r.node) {
        None | Some(ExprNode::SelfRef) => owner?.clone(),
        Some(ExprNode::Send { recv: Some(inner), method, args, .. })
            if method.as_str() == "class" && args.is_empty() && matches!(&*inner.node, ExprNode::SelfRef) =>
        {
            owner?.clone()
        }
        Some(ExprNode::Const { path }) => {
            ClassId(Symbol::from(path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("::")))
        }
        _ => return None,
    };
    if !normalized.get(&class)?.contains(attr) {
        return None;
    }
    let mut call = call_normalizer(&class, attr, args[1].clone());
    call.span = e.span;
    Some(call)
}

/// Already wrapped — the rewrite must be idempotent, since a body can be
/// visited twice (a concern's copy and its includer's).
fn is_normalizer_call(e: &Expr) -> bool {
    matches!(&*e.node, ExprNode::Send { method, .. } if method.as_str().starts_with("_normalize_"))
}

fn synthetic(node: ExprNode) -> Expr {
    Expr::new(Span::synthetic(), node)
}
