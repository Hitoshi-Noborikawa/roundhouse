//! `record.slice(:id, :name)` → `{ "id" => record.id, "name" => record.name }`.
//!
//! ActiveRecord:
//!
//! ```ruby
//! def slice(*methods)
//!   methods.flatten.index_with { |method| public_send(method) }.with_indifferent_access
//! end
//! ```
//!
//! With literal names on a receiver the analyzer typed as a model, the
//! `public_send`s are a closed set of reader calls, so every target can
//! make them as ordinary typed sends. The Hash is String-keyed, as the
//! indifferent Hash Rails builds stores its keys, and its values are
//! the union of what those readers answer. Campfire's typing channel
//! broadcasts `current_user.slice(:id, :name)`.
//!
//! A receiver the analyzer could not type (campfire's `current_user`
//! comes back from the cable connection untyped) keeps the call: the
//! ruby family answers it from the runtime's `Base#slice`, which reads
//! columns only. A model whose own code defines `slice` keeps its own.
//! A name the model does not answer is left for the ledger rather than
//! folded into a nil Rails would raise on.

use std::collections::BTreeSet;
use crate::det::HashMap;

use crate::analyze::ClassInfo;
use crate::app::App;
use crate::diagnostic::Diagnostic;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol, VarId};
use crate::ty::Ty;

pub fn apply_record_slice_lowering(app: &mut App, registry: &HashMap<ClassId, ClassInfo>) -> Vec<Diagnostic> {
    let models: BTreeSet<ClassId> = app.models.iter().map(|m| m.name.clone()).collect();
    // From source, not the registry: the catalog stamps Rails' `slice`
    // onto every model there, so the registry cannot tell it apart.
    let defines_slice = |m: &crate::dialect::MethodDef| {
        m.name.as_str() == "slice" && m.receiver == crate::dialect::MethodReceiver::Instance
    };
    let own_slice: BTreeSet<ClassId> = app
        .models
        .iter()
        .filter(|m| m.methods().any(defines_slice))
        .map(|m| m.name.clone())
        .chain(app.library_classes.iter().filter(|lc| lc.methods.iter().any(defines_slice)).map(|lc| lc.name.clone()))
        .collect();
    let ctx = Ctx { registry, models: &models, own_slice: &own_slice, next_binding: std::cell::Cell::new(0) };
    let mut diags = Vec::new();
    super::for_each_owned_hook_body(app, &mut |owner, body| rewrite(body, owner, &ctx, &mut diags));
    for view in &mut app.views {
        rewrite(&mut view.body, None, &ctx, &mut diags);
    }
    super::for_each_test_body(app, &mut |body| rewrite(body, None, &ctx, &mut diags));
    diags
}

struct Ctx<'a> {
    registry: &'a HashMap<ClassId, ClassInfo>,
    models: &'a BTreeSet<ClassId>,
    own_slice: &'a BTreeSet<ClassId>,
    /// Numbers each site's receiver binding, so no two share a name.
    next_binding: std::cell::Cell<usize>,
}

fn rewrite(expr: &mut Expr, owner: Option<&ClassId>, ctx: &Ctx, diags: &mut Vec<Diagnostic>) {
    expr.node.for_each_child_mut(&mut |c| rewrite(c, owner, ctx, diags));
    if let Some(lowered) = lower_slice(expr, owner, ctx, diags) {
        *expr = lowered;
    }
}

fn lower_slice(expr: &Expr, owner: Option<&ClassId>, ctx: &Ctx, diags: &mut Vec<Diagnostic>) -> Option<Expr> {
    let ExprNode::Send { recv, method, args, block: None, .. } = &*expr.node else { return None };
    if method.as_str() != "slice" {
        return None;
    }
    // Implicit self reads against the class whose method this is.
    let model = match recv {
        Some(r) => match r.ty.as_ref() {
            Some(Ty::Class { id, .. }) => id.clone(),
            _ => return None,
        },
        None => owner?.clone(),
    };
    if !ctx.models.contains(&model) {
        return None;
    }
    if inherits_own_slice(ctx, &model) {
        return None;
    }
    let names = literal_names(args)?;

    let span = expr.span;
    let mut readers = Vec::with_capacity(names.len());
    for name in &names {
        match super::send_dispatch::method_return_via_registry(ctx.registry, &model, name) {
            Some(ty) => readers.push((name, ty)),
            _ => {
                diags.push(super::residue_diagnostic(
                    "record_slice",
                    "record-slice",
                    span,
                    "unknown-reader",
                    format!(
                        "`slice` names `{name}`, which {} does not answer — left as a runtime call, \
                         which strict targets cannot compile",
                        model.0
                    ),
                ));
                return None;
            }
        }
    }

    // The receiver is read once, as Rails reads it, unless reading it
    // again cannot be told apart. Several reads bind it in a `Let`, a
    // value in every target, under a name unique to the site. With no
    // reads at all, an effectful receiver keeps the call: an empty Hash
    // would skip it, and a binding nothing reads does not compile in Go.
    let effect_free = recv.as_ref().map_or(true, |r| super::blank::is_effect_free_reader(r));
    if readers.is_empty() && !effect_free {
        diags.push(super::residue_diagnostic(
            "record_slice",
            "record-slice",
            span,
            "effectful-no-names",
            "`slice` with no names on a receiver with effects is left as a runtime call, \
             which strict targets cannot compile; it answers `{}`, so drop the call or name the columns"
                .to_string(),
        ));
        return None;
    }
    let (binding, read) = match recv {
        None => (None, None),
        Some(r) if readers.len() == 1 || effect_free => (None, Some(r.clone())),
        Some(r) => {
            let name = Symbol::from(format!("slice_record_{}", ctx.next_binding.get()));
            ctx.next_binding.set(ctx.next_binding.get() + 1);
            let mut read = Expr::new(span, ExprNode::Var { id: VarId(0), name: name.clone() });
            read.ty = r.ty.clone();
            (Some((name, r.clone())), Some(read))
        }
    };

    let mut value_ty = Ty::Bottom;
    let entries = readers
        .into_iter()
        .map(|(name, ty)| {
            value_ty = crate::analyze::union_of(value_ty.clone(), ty.clone());
            let mut key = Expr::new(span, ExprNode::Lit { value: Literal::Str { value: name.clone() } });
            key.ty = Some(Ty::Str);
            let mut value = Expr::new(
                span,
                ExprNode::Send {
                    recv: read.clone(),
                    method: Symbol::from(name.as_str()),
                    args: Vec::new(),
                    block: None,
                    parenthesized: false,
                },
            );
            value.ty = Some(ty);
            (key, value)
        })
        .collect();
    let hash_ty = Ty::Hash {
        key: std::sync::Arc::new(Ty::Str),
        value: std::sync::Arc::new(if matches!(value_ty, Ty::Bottom) { Ty::Untyped } else { value_ty }),
    };
    let mut literal = Expr::new(span, ExprNode::Hash { entries, kwargs: false });
    literal.ty = Some(hash_ty.clone());
    // A Cast to the literal's own type: the values need not share one,
    // and a target whose Hash literal unifies them by its first entry
    // (rust's `HashMap::from`) builds it entry by entry under a Cast.
    let mut hash = Expr::new(span, ExprNode::Cast { value: literal, target_ty: hash_ty.clone() });
    hash.ty = Some(hash_ty);
    Some(match binding {
        None => hash,
        Some((name, value)) => {
            let ty = hash.ty.clone();
            let mut bound = Expr::new(span, ExprNode::Let { id: VarId(0), name, value, body: hash });
            bound.ty = ty;
            bound
        }
    })
}

/// Does `model`, a parent of it, or a module it includes define `slice`
/// in the app's own code?
fn inherits_own_slice(ctx: &Ctx, model: &ClassId) -> bool {
    let mut seen: BTreeSet<ClassId> = BTreeSet::new();
    let mut stack = vec![model.clone()];
    while let Some(cid) = stack.pop() {
        if !seen.insert(cid.clone()) {
            continue;
        }
        if ctx.own_slice.contains(&cid) {
            return true;
        }
        if let Some(cls) = ctx.registry.get(&cid) {
            stack.extend(cls.includes.iter().cloned());
            stack.extend(cls.parent.iter().cloned());
        }
    }
    false
}

/// `slice(:a, "b")` or Rails' flattened `slice([:a, :b])`; `None` for a
/// name only known at run time.
fn literal_names(args: &[Expr]) -> Option<Vec<String>> {
    let mut names = Vec::new();
    for arg in args {
        match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } => names.push(value.as_str().to_string()),
            ExprNode::Lit { value: Literal::Str { value } } => names.push(value.clone()),
            ExprNode::Array { elements, .. } => names.extend(literal_names(elements)?),
            _ => return None,
        }
    }
    Some(names)
}
