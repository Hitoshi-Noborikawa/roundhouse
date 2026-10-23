// Not a fixed hook list like `CallbackHook`: an app or gem names its own events, so each chain is built as Rails builds it.

use std::collections::HashMap;

use crate::dialect::{MethodReceiver, Model, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};

const RUNTIME_EVENTS: [&str; 5] = ["save", "create", "update", "destroy", "validation"];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Before,
    After,
    Around,
}

#[derive(Clone)]
enum Target {
    Method(Symbol),
    Block(String),
}

#[derive(Clone)]
struct Link {
    kind: Kind,
    target: Target,
    condition: Option<String>,
}

#[derive(Clone, Default)]
struct Event {
    model: bool,
    allowed: Vec<&'static str>,
    chain: Vec<Link>,
    // Not built at all when one declaration cannot be read: a chain missing a link answers wrongly.
    refused: bool,
}

fn sym(e: &Expr) -> Option<Symbol> {
    match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.clone()),
        _ => None,
    }
}

fn kind_of(s: &str) -> Option<Kind> {
    match s {
        "before" => Some(Kind::Before),
        "after" => Some(Kind::After),
        "around" => Some(Kind::Around),
        _ => None,
    }
}

fn inline(lambda: &Expr) -> Option<String> {
    let ExprNode::Lambda { params, rest_param: None, body, .. } = &*lambda.node else { return None };
    let body = crate::emit::ruby::expr::emit_expr(body);
    match params.as_slice() {
        [] => Some(body),
        [p] => Some(format!("{} = self\n{body}", p.as_str())),
        _ => None,
    }
}

fn condition_of(value: &Expr) -> Option<String> {
    match &*value.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
        ExprNode::Lambda { .. } => inline(value).map(|body| format!("(begin\n{body}\nend)")),
        ExprNode::Array { elements, .. } => {
            let parts: Option<Vec<String>> = elements.iter().map(condition_of).collect();
            parts.map(|p| p.join(" && "))
        }
        _ => None,
    }
}

fn options(args: &mut Vec<Expr>) -> Option<(Option<String>, bool)> {
    let Some(ExprNode::Hash { entries, kwargs: true }) = args.last().map(|a| &*a.node) else { return Some((None, false)) };
    let entries = entries.clone();
    args.pop();
    let mut conds: Vec<String> = Vec::new();
    let mut prepend = false;
    for (k, v) in &entries {
        match sym(k)?.as_str() {
            "if" => conds.push(condition_of(v)?),
            "unless" => conds.push(format!("!({})", condition_of(v)?)),
            "prepend" => prepend = matches!(&*v.node, ExprNode::Lit { value: Literal::Bool { value: true } }),
            _ => return None,
        }
    }
    Some(((!conds.is_empty()).then(|| conds.join(" && ")), prepend))
}

fn targets(args: &[Expr], block: Option<&Expr>) -> Option<Vec<Target>> {
    let mut out: Vec<Target> = Vec::new();
    for a in args {
        match &*a.node {
            ExprNode::Lit { value: Literal::Sym { value } } => out.push(Target::Method(value.clone())),
            ExprNode::Lambda { .. } => out.push(Target::Block(inline(a)?)),
            _ => return None,
        }
    }
    if let Some(b) = block {
        out.push(Target::Block(inline(b)?));
    }
    (!out.is_empty()).then_some(out)
}

fn read(expr: &Expr, events: &mut indexmap::IndexMap<Symbol, Event>) -> bool {
    let ExprNode::Send { recv: None, method, args, block, .. } = &*expr.node else { return false };
    let name = method.as_str();
    let mut args = args.clone();
    match name {
        "define_model_callbacks" | "define_callbacks" => {
            let mut allowed = vec!["before", "after", "around"];
            if let Some(ExprNode::Hash { entries, kwargs: true }) = args.last().map(|a| &*a.node) {
                let mut ok = name == "define_model_callbacks";
                for (k, v) in entries {
                    match (sym(k).as_ref().map(|s| s.as_str()), &*v.node) {
                        (Some("only"), ExprNode::Array { elements, .. }) => {
                            allowed = elements.iter().filter_map(sym).filter_map(|s| ["before", "after", "around"].into_iter().find(|k| *k == s.as_str())).collect();
                        }
                        (Some("only"), ExprNode::Lit { value: Literal::Sym { value } }) => {
                            allowed = ["before", "after", "around"].into_iter().filter(|k| *k == value.as_str()).collect();
                        }
                        _ => ok = false,
                    }
                }
                args.pop();
                if !ok {
                    for a in &args {
                        if let Some(s) = sym(a) {
                            events.entry(s).or_default().refused = true;
                        }
                    }
                    return true;
                }
            }
            for a in &args {
                let Some(s) = sym(a) else { continue };
                let ev = events.entry(s).or_default();
                ev.model = name == "define_model_callbacks";
                ev.allowed = allowed.clone();
            }
            true
        }
        "set_callback" | "skip_callback" => {
            let (Some(event), Some(kind)) = (args.first().and_then(sym), args.get(1).and_then(sym).and_then(|k| kind_of(k.as_str()))) else {
                return false;
            };
            if !events.contains_key(&event) {
                return false;
            }
            let rest: Vec<Expr> = args[2..].to_vec();
            let mut rest = rest;
            let parsed = options(&mut rest).and_then(|(cond, prepend)| targets(&rest, block.as_ref()).map(|t| (cond, prepend, t)));
            let ev = events.get_mut(&event).expect("checked");
            match parsed {
                Some((cond, prepend, ts)) if name == "set_callback" => {
                    let links: Vec<Link> = ts.into_iter().map(|target| Link { kind, target, condition: cond.clone() }).collect();
                    // Not inserted as a block: Rails prepends each filter in turn, so one call's filters land reversed.
                    if prepend {
                        for l in links {
                            ev.chain.insert(0, l);
                        }
                    } else {
                        ev.chain.extend(links);
                    }
                }
                Some((None, _, ts)) => {
                    ev.chain.retain(|l| {
                        !(l.kind == kind && ts.iter().any(|t| matches!((t, &l.target), (Target::Method(a), Target::Method(b)) if a == b)))
                    });
                }
                _ => ev.refused = true,
            }
            true
        }
        _ => {
            let Some((kind_name, event)) = name.split_once('_') else { return false };
            let Some(kind) = kind_of(kind_name) else { return false };
            let event = Symbol::from(event);
            let Some(ev) = events.get_mut(&event) else { return false };
            if !ev.model || !ev.allowed.contains(&kind_name) {
                return false;
            }
            match options(&mut args).and_then(|(cond, prepend)| targets(&args, block.as_ref()).map(|t| (cond, prepend, t))) {
                Some((cond, prepend, ts)) => {
                    let links: Vec<Link> = ts.into_iter().map(|target| Link { kind, target, condition: cond.clone() }).collect();
                    // Not appended: Rails prepends a model after, so the afters run last in the order written.
                    if kind == Kind::After || prepend {
                        for l in links {
                            ev.chain.insert(0, l);
                        }
                    } else {
                        ev.chain.extend(links);
                    }
                }
                None => ev.refused = true,
            }
            true
        }
    }
}

fn is_definition(expr: &Expr) -> bool {
    matches!(&*expr.node, ExprNode::Send { recv: None, method, .. } if matches!(method.as_str(), "define_model_callbacks" | "define_callbacks"))
}

fn call(target: &Target) -> String {
    match target {
        Target::Method(m) => m.as_str().to_string(),
        Target::Block(body) => format!("begin\n{body}\nend"),
    }
}

fn guarded(link: &Link, code: String) -> String {
    match &link.condition {
        Some(c) => format!("if {c}\n{code}\nend"),
        None => code,
    }
}

fn nest(chain: &[Link], body: &str) -> String {
    let Some((first, rest)) = chain.split_first() else { return body.to_string() };
    let inner = nest(rest, body);
    match first.kind {
        Kind::Before => format!("{}\n{inner}", guarded(first, call(&first.target))),
        Kind::After => format!("{inner}\n{}", guarded(first, call(&first.target))),
        Kind::Around => {
            let wrapped = match &first.target {
                Target::Method(m) => format!("{} do\n{inner}\nend", m.as_str()),
                Target::Block(_) => return String::new(),
            };
            match &first.condition {
                Some(c) => format!("if {c}\n{wrapped}\nelse\n{inner}\nend"),
                None => wrapped,
            }
        }
    }
}

fn source(events: &indexmap::IndexMap<Symbol, Event>, halting: impl Fn(&Event) -> bool) -> String {
    let mut src = String::from("class CallbacksSynth < ApplicationRecord\n");
    for (name, ev) in events {
        let body = "__rc_value = block_given? ? yield : true";
        let code = if halting(ev) {
            format!(
                "__rc_value = nil\n__rc_done = false\n__rc_result = catch(:abort) do\n{}\n__rc_done = true\n__rc_value\nend\n__rc_done ? __rc_result : false",
                nest(&ev.chain, body)
            )
        } else {
            format!("__rc_value = nil\n{}\n__rc_value", nest(&ev.chain, body))
        };
        src.push_str(&format!("def _run_{}_callbacks\n{code}\nend\n", name.as_str()));
    }
    src.push_str("end\n");
    src
}

fn throws(e: &Expr) -> bool {
    if matches!(&*e.node, ExprNode::Send { method, .. } if method.as_str() == "throw") {
        return true;
    }
    let mut found = false;
    e.node.for_each_child(&mut |c| found |= throws(c));
    found
}

fn rewrite(e: &mut Expr) {
    if let ExprNode::Send { method, args, .. } = &mut *e.node
        && method.as_str() == "run_callbacks"
        && let [only] = args.as_slice()
        && let Some(event) = sym(only)
    {
        *method = Symbol::from(format!("_run_{}_callbacks", event.as_str()));
        args.clear();
    }
    e.node.for_each_child_mut(&mut rewrite);
}

pub(super) fn expand_model_callbacks(app: &mut crate::App) {
    let parents: HashMap<ClassId, ClassId> = app.models.iter().filter_map(|m| Some((m.name.clone(), m.parent.clone()?))).collect();
    let mut declared: HashMap<ClassId, indexmap::IndexMap<Symbol, Event>> = HashMap::new();
    let mut order: Vec<usize> = (0..app.models.len()).collect();
    let depth = |m: &Model| {
        let mut d = 0;
        let mut cur = m.parent.clone();
        while let Some(p) = cur {
            d += 1;
            cur = parents.get(&p).cloned();
            if d > 32 {
                break;
            }
        }
        d
    };
    order.sort_by_key(|&i| depth(&app.models[i]));
    for i in order {
        let model = &app.models[i];
        let mut events = model.parent.as_ref().and_then(|p| declared.get(p)).cloned().unwrap_or_default();
        let before = events.len();
        let mut consumed: Vec<usize> = Vec::new();
        // Not in source order: a mixin's `define_model_callbacks` can land after the model's own `after_x`.
        for defining in [true, false] {
            for (at, item) in model.body.iter().enumerate() {
                if let ModelBodyItem::Unknown { expr, .. } = item
                    && is_definition(expr) == defining
                    && read(expr, &mut events)
                {
                    consumed.push(at);
                }
            }
        }
        consumed.sort_unstable();
        if events.is_empty() || (consumed.is_empty() && events.len() == before) {
            if !events.is_empty() {
                declared.insert(model.name.clone(), events);
            }
            continue;
        }
        let name = model.name.clone();
        for (event, _) in events.iter().filter(|(_, ev)| ev.refused) {
            super::prism::push(crate::diagnostic::Diagnostic::unsupported(
                model.body.get(*consumed.first().unwrap_or(&0)).map(|i| i.span()).unwrap_or_default(),
                None,
                "callback",
                format!("a `{}` callback on `{}` is declared in a form this does not read (an option, or a target that is not a Symbol, a block or a lambda), so its chain is not built", event.as_str(), name.0.as_str()),
            ));
        }
        let methods: HashMap<Symbol, &crate::dialect::MethodDef> =
            model.methods().filter(|m| m.receiver == MethodReceiver::Instance).map(|m| (m.name.clone(), m)).collect();
        // Not caught when nothing in the chain can throw: a target the model does not define might.
        let halting = |ev: &Event| {
            ev.chain.iter().filter(|l| l.kind != Kind::After).any(|l| match &l.target {
                Target::Method(m) => methods.get(m).is_none_or(|d| throws(&d.body)),
                Target::Block(b) => b.contains("throw"),
            })
        };
        let buildable: indexmap::IndexMap<Symbol, Event> = events
            .iter()
            .filter(|(e, ev)| !ev.refused && !RUNTIME_EVENTS.contains(&e.as_str()))
            .filter(|(_, ev)| !ev.chain.iter().any(|l| l.kind == Kind::Around && matches!(l.target, Target::Block(_))))
            .map(|(e, ev)| (e.clone(), ev.clone()))
            .collect();
        for (event, _) in events.iter().filter(|(_, ev)| ev.chain.iter().any(|l| l.kind == Kind::Around && matches!(l.target, Target::Block(_)))) {
            super::prism::push(crate::diagnostic::Diagnostic::unsupported(
                Default::default(),
                None,
                "callback",
                format!("an `around_{}` block on `{}` needs its block to take the chain as a proc, which this does not build", event.as_str(), name.0.as_str()),
            ));
        }
        let src = source(&buildable, halting);
        let (parsed, diags) = super::prism::scope(|| super::model::ingest_model(src.as_bytes(), "<callbacks>", &app.schema, &Default::default()));
        let synth = match parsed {
            Ok(Some(m)) if diags.is_empty() => m,
            _ => {
                super::survey::record_synthesis_failure("<callbacks>", &format!("callback chains for `{}`", name.0.as_str()), &diags);
                declared.insert(name, events);
                continue;
            }
        };
        let model = &mut app.models[i];
        for at in consumed.into_iter().rev() {
            model.body.remove(at);
        }
        model.body.extend(synth.body.into_iter().filter(|it| matches!(it, ModelBodyItem::Method { .. })));
        declared.insert(name, events);
    }
    // Not only models: an STI subclass is a library class whose parent is one, and its body may skip or add a link.
    let mut pending: Vec<usize> = (0..app.library_classes.len()).filter(|&i| !app.library_classes[i].is_module).collect();
    loop {
        let before = pending.len();
        pending.retain(|&i| {
            let lc = &app.library_classes[i];
            let Some(events) = lc.parent.as_ref().and_then(|p| declared.get(p)).cloned() else { return true };
            let mut events = events;
            let mut consumed: Vec<usize> = Vec::new();
            for (at, call) in lc.unknown_calls.iter().enumerate() {
                if read(call, &mut events) {
                    consumed.push(at);
                }
            }
            let name = lc.name.clone();
            if !consumed.is_empty() {
                let methods: HashMap<Symbol, crate::dialect::MethodDef> =
                    lc.methods.iter().filter(|m| m.receiver == MethodReceiver::Instance).map(|m| (m.name.clone(), m.clone())).collect();
                let halting = |ev: &Event| {
                    ev.chain.iter().filter(|l| l.kind != Kind::After).any(|l| match &l.target {
                        Target::Method(m) => methods.get(m).is_none_or(|d| throws(&d.body)),
                        Target::Block(b) => b.contains("throw"),
                    })
                };
                let buildable: indexmap::IndexMap<Symbol, Event> = events
                    .iter()
                    .filter(|(e, ev)| !ev.refused && !RUNTIME_EVENTS.contains(&e.as_str()))
                    .map(|(e, ev)| (e.clone(), ev.clone()))
                    .collect();
                let src = source(&buildable, halting);
                let (parsed, diags) = super::prism::scope(|| super::model::ingest_model(src.as_bytes(), "<callbacks>", &app.schema, &Default::default()));
                match parsed {
                    Ok(Some(m)) if diags.is_empty() => {
                        let lc = &mut app.library_classes[i];
                        for at in consumed.into_iter().rev() {
                            lc.unknown_calls.remove(at);
                        }
                        lc.methods.extend(m.methods().cloned().map(|mut m| {
                            m.enclosing_class = Some(name.0.clone());
                            m
                        }));
                    }
                    _ => super::survey::record_synthesis_failure("<callbacks>", &format!("callback chains for `{}`", name.0.as_str()), &diags),
                }
            }
            declared.insert(name, events);
            false
        });
        if pending.len() == before {
            break;
        }
    }
    rewrite_run_callbacks(app);
}

fn rewrite_run_callbacks(app: &mut crate::App) {
    crate::lower::for_each_hook_body(app, &mut rewrite);
}
