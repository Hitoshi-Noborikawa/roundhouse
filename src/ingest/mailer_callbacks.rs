//! A mailer's `before_action` / `after_action` / `around_action`.
//! actionmailer 8.1.4 runs them as one chain, in declaration order and
//! parent class first: a before callback runs and then the rest, an
//! around callback runs with the rest as its block, and an after callback
//! runs the rest and then itself, so afters run in reverse. The action
//! ends the chain. `only:` / `except:` pick actions by name, `if:` /
//! `unless:` take a Symbol or a lambda, and `with_options` merges its
//! options into the calls in its block. Nothing halts the chain
//! (`throw :abort` isn't caught).
//!
//! On the ruby family ([`expand`]) each public action of a mailer with a
//! chain keeps its name and gains the chain, resolved for that action,
//! as source ingested like the class's own:
//!
//! ```ruby
//! def hello(a)
//!   __start_action("hello")
//!   load_params
//!   wrap do
//!     __mailer_action_hello(a)   # the action's own body, now private
//!     after_one
//!   end
//!   message
//! end
//! ```
//!
//! A block callback and a lambda condition become private methods of
//! the class that declares them. The chain runs when the action is
//! called (`Mailer.hello(...)`), not when the mail is delivered: the
//! runtime builds the mail eagerly, so a callback's errors and side
//! effects happen at that call rather than at `deliver_now` or
//! `.message`.
//!
//! A chain it doesn't take (`skip_before_action`, a non-literal option,
//! a callback method the class doesn't define, an action with keyword or
//! block parameters, a name the generated methods would take) keeps the
//! class's calls reported, and its subclasses' with it. Other targets
//! report every callback ([`refuse`]).

use std::collections::{BTreeSet, HashMap};

use crate::app::App;
use crate::dialect::{LibraryClass, MethodReceiver, MethodVisibility};
use crate::expr::{Expr, ExprNode, Literal};
use crate::span::Span;

const ACTION_PREFIX: &str = "__mailer_action_";
const CALLBACK_PREFIX: &str = "__mailer_callback_";
const CONDITION_PREFIX: &str = "__mailer_condition_";

/// The action a mailer method answers for: its own name, or the action
/// whose body it holds under the chain.
pub(crate) fn action_of(name: &str) -> &str {
    name.strip_prefix(ACTION_PREFIX).unwrap_or(name)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Before,
    After,
    Around,
}

#[derive(Clone)]
enum Call {
    /// A method of the mailer, by name.
    Method(String),
    /// A block or lambda body, generated as a private method.
    Body(String, Expr),
}

#[derive(Clone)]
struct Callback {
    kind: Kind,
    target: Call,
    only: Option<Vec<String>>,
    except: Option<Vec<String>>,
    ifs: Vec<Call>,
    unlesses: Vec<Call>,
}

impl Callback {
    fn applies_to(&self, action: &str) -> bool {
        self.only.as_ref().is_none_or(|names| names.iter().any(|n| n == action))
            && !self.except.as_ref().is_some_and(|names| names.iter().any(|n| n == action))
    }
}

/// One class's callbacks, the unknown calls they came from, and the
/// generated bodies it declares.
struct Parsed {
    callbacks: Vec<Callback>,
    consumed: Vec<usize>,
}

fn kind_of(method: &str) -> Option<Kind> {
    match method {
        "before_action" => Some(Kind::Before),
        "after_action" => Some(Kind::After),
        "around_action" => Some(Kind::Around),
        _ => None,
    }
}

fn is_callback_macro(method: &str) -> bool {
    kind_of(method).is_some()
        || matches!(
            method,
            "skip_before_action"
                | "skip_after_action"
                | "skip_around_action"
                | "prepend_before_action"
                | "prepend_after_action"
                | "prepend_around_action"
                | "append_before_action"
                | "append_after_action"
                | "append_around_action"
        )
}

/// The calls in a `with_options` block, or None when it isn't one.
fn with_options_calls(call: &Expr) -> Option<(&[(Expr, Expr)], Vec<&Expr>)> {
    let ExprNode::Send { recv: None, method, args, block: Some(block), .. } = &*call.node else { return None };
    if method.as_str() != "with_options" {
        return None;
    }
    let [hash] = args.as_slice() else { return None };
    let ExprNode::Hash { entries, .. } = &*hash.node else { return None };
    let ExprNode::Lambda { params, body, .. } = &*block.node else { return None };
    if !params.is_empty() {
        return None;
    }
    let stmts: Vec<&Expr> = match &*body.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![body],
    };
    Some((entries, stmts))
}

fn callback_method(call: &Expr) -> Option<&str> {
    match &*call.node {
        ExprNode::Send { recv: None, method, .. } if is_callback_macro(method.as_str()) => Some(method.as_str()),
        _ => None,
    }
}

/// Does this unknown call declare (or merge into) a callback?
fn declares_callbacks(call: &Expr) -> bool {
    callback_method(call).is_some()
        || with_options_calls(call).is_some_and(|(_, stmts)| stmts.iter().any(|s| callback_method(s).is_some()))
}

fn names(value: &Expr) -> Option<Vec<String>> {
    let one = |e: &Expr| match &*e.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Some(value.as_str().to_string()),
        ExprNode::Lit { value: Literal::Str { value } } => Some(value.to_string()),
        _ => None,
    };
    match &*value.node {
        ExprNode::Array { elements, .. } => elements.iter().map(one).collect(),
        _ => one(value).map(|n| vec![n]),
    }
}

fn condition(value: &Expr, counter: &mut usize) -> Result<Call, String> {
    match &*value.node {
        ExprNode::Lit { value: Literal::Sym { value } } => Ok(Call::Method(value.as_str().to_string())),
        ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. }
            if params.is_empty() && extra_params.is_empty() =>
        {
            *counter += 1;
            Ok(Call::Body(format!("{CONDITION_PREFIX}{counter}"), body.clone()))
        }
        _ => Err("a condition other than a Symbol or a lambda without parameters".into()),
    }
}

/// One `before_action` (…) call, with a `with_options` block's options
/// under its own.
fn parse_call(call: &Expr, outer: &[(Expr, Expr)], counter: &mut usize) -> Result<Vec<Callback>, String> {
    let ExprNode::Send { method, args, block, .. } = &*call.node else { unreachable!() };
    let Some(kind) = kind_of(method.as_str()) else {
        return Err(format!("`{}` is not lowered", method.as_str()));
    };
    let mut targets: Vec<Call> = Vec::new();
    let mut options: Vec<(Expr, Expr)> = outer.to_vec();
    for arg in args {
        match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } => targets.push(Call::Method(value.as_str().to_string())),
            ExprNode::Hash { entries, .. } => options.extend(entries.iter().cloned()),
            _ => return Err("a callback other than a Symbol or a block".into()),
        }
    }
    if let Some(block) = block {
        let ExprNode::Lambda { params, rest_param: None, extra_params, block_param: None, body, .. } = &*block.node
        else {
            return Err("a block callback with parameters".into());
        };
        if !params.is_empty() || !extra_params.is_empty() || !targets.is_empty() || kind == Kind::Around {
            return Err("a block callback with parameters, or beside named callbacks".into());
        }
        *counter += 1;
        targets.push(Call::Body(format!("{CALLBACK_PREFIX}{counter}"), body.clone()));
    }
    if targets.is_empty() {
        return Err("a callback call naming no callback".into());
    }
    let (mut only, mut except, mut ifs, mut unlesses) = (None, None, Vec::new(), Vec::new());
    // A later key wins, as a Hash merge does.
    for (key, value) in &options {
        let ExprNode::Lit { value: Literal::Sym { value: key } } = &*key.node else {
            return Err("a non-Symbol option key".into());
        };
        match key.as_str() {
            "only" => only = Some(names(value).ok_or("an `only:` other than literal names")?),
            "except" => except = Some(names(value).ok_or("an `except:` other than literal names")?),
            "if" => ifs = vec![condition(value, counter)?],
            "unless" => unlesses = vec![condition(value, counter)?],
            other => return Err(format!("the `{other}:` option")),
        }
    }
    Ok(targets
        .into_iter()
        .map(|target| Callback { kind, target, only: only.clone(), except: except.clone(), ifs: ifs.clone(), unlesses: unlesses.clone() })
        .collect())
}

fn parse_class(lc: &LibraryClass, counter: &mut usize) -> Result<Parsed, String> {
    let mut parsed = Parsed { callbacks: Vec::new(), consumed: Vec::new() };
    for (i, call) in lc.unknown_calls.iter().enumerate() {
        if callback_method(call).is_some() {
            parsed.callbacks.extend(parse_call(call, &[], counter)?);
            parsed.consumed.push(i);
        } else if let Some((outer, stmts)) = with_options_calls(call) {
            if !stmts.iter().any(|s| callback_method(s).is_some()) {
                continue;
            }
            if !stmts.iter().all(|s| callback_method(s).is_some()) {
                return Err("a `with_options` block with callbacks and other calls".into());
            }
            for stmt in stmts {
                parsed.callbacks.extend(parse_call(stmt, outer, counter)?);
            }
            parsed.consumed.push(i);
        }
    }
    Ok(parsed)
}

/// The ruby family: each mailer's actions gain their chains. Returns
/// the warnings for the chains it doesn't take.
pub(crate) fn expand(app: &mut App, target: &str) -> Vec<crate::diagnostic::Diagnostic> {
    let mailers = crate::lower::mailer_class_side::mailer_names(app);
    if mailers.is_empty() {
        return Vec::new();
    }
    let parents: HashMap<String, String> = app
        .library_classes
        .iter()
        .filter_map(|lc| lc.parent.as_ref().map(|p| (lc.name.0.as_str().to_string(), p.0.as_str().to_string())))
        .collect();
    // Mailers nearest the base first, so a parent is decided before its
    // subclasses.
    let depth = |name: &str| {
        let mut n = 0;
        let mut cur = name.to_string();
        while let Some(p) = parents.get(&cur).filter(|p| mailers.contains(*p)) {
            n += 1;
            cur = p.clone();
            if n > 64 {
                break;
            }
        }
        n
    };
    let mut order: Vec<usize> =
        (0..app.library_classes.len()).filter(|&i| mailers.contains(app.library_classes[i].name.0.as_str())).collect();
    order.sort_by_key(|&i| depth(app.library_classes[i].name.0.as_str()));

    let mut counter = 0usize;
    // Actions given a chain, by class: a callback naming one calls its own
    // body, as Rails calls the method, not the chain again.
    let mut wrapped: HashMap<String, BTreeSet<String>> = HashMap::new();
    // Every name the generated chains took.
    let mut generated: BTreeSet<String> = BTreeSet::new();
    let mut chains: HashMap<String, Vec<Callback>> = HashMap::new();
    let mut refused: BTreeSet<String> = BTreeSet::new();
    let mut gaps: Vec<(Span, String, String)> = Vec::new();
    for i in order {
        let name = app.library_classes[i].name.0.as_str().to_string();
        let parent = parents.get(&name).filter(|p| mailers.contains(*p)).cloned();
        let decided = (|| -> Result<Option<Vec<crate::dialect::MethodDef>>, String> {
            if parent.as_ref().is_some_and(|p| refused.contains(p)) {
                return Err("its parent mailer's callbacks aren't lowered".into());
            }
            let parsed = parse_class(&app.library_classes[i], &mut counter)?;
            let mut chain = parent.as_ref().and_then(|p| chains.get(p)).cloned().unwrap_or_default();
            chain.extend(parsed.callbacks.iter().cloned());
            chains.insert(name.clone(), chain.clone());
            if chain.is_empty() {
                return Ok(None);
            }
            let methods = generated_methods(app, i, &chain, &parsed, &parents, &mailers, &wrapped, &generated)?;
            Ok(Some(methods))
        })();
        match decided {
            Ok(None) => {}
            Ok(Some(methods)) => {
                let lc = &mut app.library_classes[i];
                let actions: Vec<String> = actions(lc);
                wrapped.insert(name.clone(), actions.iter().cloned().collect());
                generated.extend(actions.iter().map(|a| format!("{ACTION_PREFIX}{a}")));
                generated.extend(
                    methods
                        .iter()
                        .map(|m| m.name.as_str().to_string())
                        .filter(|n| n.starts_with(CALLBACK_PREFIX) || n.starts_with(CONDITION_PREFIX)),
                );
                for m in lc.methods.iter_mut() {
                    if m.receiver == MethodReceiver::Instance && actions.contains(&m.name.as_str().to_string()) {
                        m.name = crate::ident::Symbol::from(format!("{ACTION_PREFIX}{}", m.name.as_str()));
                        m.visibility = MethodVisibility::Private;
                    }
                }
                lc.methods.extend(methods);
                let consumed: Vec<usize> = lc
                    .unknown_calls
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| declares_callbacks(c))
                    .map(|(i, _)| i)
                    .collect();
                remove_calls(lc, &consumed);
            }
            Err(reason) => {
                refused.insert(name.clone());
                let lc = &mut app.library_classes[i];
                let mine: Vec<usize> =
                    lc.unknown_calls.iter().enumerate().filter(|(_, c)| declares_callbacks(c)).map(|(i, _)| i).collect();
                for &k in &mine {
                    gaps.push((lc.unknown_calls[k].span, construct(&lc.unknown_calls[k]), reason.clone()));
                }
                remove_calls(lc, &mine);
            }
        }
    }
    let mut out = Vec::new();
    for (span, construct, reason) in gaps {
        out.extend(super::app::validates_gap_diagnostics(
            app,
            target,
            vec![(span, construct)],
            &format!("mailer callback not lowered: {reason}"),
        ));
    }
    out
}

/// A target without the chain: every mailer callback is reported, and
/// its action runs without it, as before.
pub(crate) fn refuse(app: &mut App, target: &str) -> Vec<crate::diagnostic::Diagnostic> {
    let mailers = crate::lower::mailer_class_side::mailer_names(app);
    let mut gaps: Vec<(Span, String)> = Vec::new();
    for lc in &app.library_classes {
        if !mailers.contains(lc.name.0.as_str()) {
            continue;
        }
        for call in lc.unknown_calls.iter().filter(|c| declares_callbacks(c)) {
            gaps.push((call.span, construct(call)));
        }
    }
    super::app::validates_gap_diagnostics(
        app,
        target,
        gaps,
        "mailer callback not lowered: this target has no mailer callback chain, so the action runs without it",
    )
}

fn construct(call: &Expr) -> String {
    match &*call.node {
        ExprNode::Send { method, .. } => format!("mailer {}", method.as_str()),
        _ => "mailer callback".into(),
    }
}

fn remove_calls(lc: &mut LibraryClass, indices: &[usize]) {
    let mut i = 0;
    lc.unknown_calls.retain(|_| {
        let keep = !indices.contains(&i);
        i += 1;
        keep
    });
}

/// The class's own public actions: what ActionMailer's class-side proxy
/// answers (`action_methods`).
fn actions(lc: &LibraryClass) -> Vec<String> {
    lc.methods
        .iter()
        .filter(|m| {
            m.receiver == MethodReceiver::Instance
                && m.visibility == MethodVisibility::Public
                && m.name.as_str() != "initialize"
                && !m.name.as_str().starts_with("__")
        })
        .map(|m| m.name.as_str().to_string())
        .collect()
}

/// Every instance method the class and its mailer ancestors define, or
/// a module they include.
fn defined(app: &App, i: usize, parents: &HashMap<String, String>, mailers: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = Some(app.library_classes[i].name.0.as_str().to_string());
    let mut guard = 0;
    while let Some(name) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        if let Some(lc) = app.library_classes.iter().find(|c| c.name.0.as_str() == name) {
            out.extend(lc.methods.iter().filter(|m| m.receiver == MethodReceiver::Instance).map(|m| m.name.as_str().to_string()));
            // A module it includes (a mailer concern), and the modules that
            // one includes.
            let mut modules: Vec<String> = lc.includes.iter().map(|id| id.0.as_str().to_string()).collect();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            while let Some(module) = modules.pop() {
                if !seen.insert(module.clone()) {
                    continue;
                }
                if let Some(m) = app.library_classes.iter().find(|c| c.is_module && c.name.0.as_str() == module) {
                    out.extend(m.methods.iter().filter(|d| d.receiver == MethodReceiver::Instance).map(|d| d.name.as_str().to_string()));
                    modules.extend(m.includes.iter().map(|id| id.0.as_str().to_string()));
                }
            }
        }
        cur = parents.get(&name).filter(|p| mailers.contains(*p)).cloned();
    }
    out
}

fn generated_methods(
    app: &App,
    i: usize,
    chain: &[Callback],
    parsed: &Parsed,
    parents: &HashMap<String, String>,
    mailers: &BTreeSet<String>,
    wrapped: &HashMap<String, BTreeSet<String>>,
    generated: &BTreeSet<String>,
) -> Result<Vec<crate::dialect::MethodDef>, String> {
    let lc = &app.library_classes[i];
    let defined = defined(app, i, parents, mailers);
    // The ancestors' actions with a chain: their bodies now carry the
    // prefix, and a callback naming one calls the body.
    let mut ancestors_wrapped: BTreeSet<String> = BTreeSet::new();
    let mut cur = parents.get(lc.name.0.as_str()).filter(|p| mailers.contains(*p)).cloned();
    while let Some(p) = cur {
        if let Some(names) = wrapped.get(&p) {
            ancestors_wrapped.extend(names.iter().cloned());
        }
        cur = parents.get(&p).filter(|q| mailers.contains(*q) && **q != p).cloned();
    }
    let own_actions: BTreeSet<String> = actions(lc).into_iter().collect();
    // A name the generated methods take must not be the app's own (one an
    // ancestor's chain generated is ours).
    if let Some(taken) = defined.iter().find(|n| {
        (n.starts_with(ACTION_PREFIX) || n.starts_with(CALLBACK_PREFIX) || n.starts_with(CONDITION_PREFIX))
            && !generated.contains(*n)
    }) {
        return Err(format!("the class defines `{taken}`, a name the generated chain would take"));
    }
    let mut generated_bodies: Vec<(String, Expr)> = Vec::new();
    let callable = |call: &Call| -> Result<String, String> {
        match call {
            Call::Method(name) if own_actions.contains(name) || ancestors_wrapped.contains(name) => {
                Ok(format!("{ACTION_PREFIX}{name}"))
            }
            Call::Method(name) if defined.contains(name) => Ok(name.clone()),
            Call::Method(name) => Err(format!("the callback `{name}` is not a method the mailer defines")),
            Call::Body(name, _) => Ok(name.clone()),
        }
    };
    let mut text = format!("class {}\n", lc.name.0.as_str());
    for action in actions(lc) {
        let m = lc
            .methods
            .iter()
            .find(|m| m.receiver == MethodReceiver::Instance && m.name.as_str() == action)
            .expect("an action is a method");
        if m.block_param.is_some() || m.params.iter().any(|p| p.keyword || p.forwarding || p.rest || p.default.is_some()) {
            return Err(format!("the action `{action}` takes other than required positional parameters"));
        }
        let args = m.params.iter().map(|p| p.name.as_str().to_string()).collect::<Vec<_>>().join(", ");
        let applies: Vec<&Callback> = chain.iter().filter(|c| c.applies_to(&action)).collect();
        let mut resolved = Vec::new();
        for c in applies {
            let target = callable(&c.target)?;
            let mut conds = Vec::new();
            for cond in &c.ifs {
                conds.push(format!("({})", callable(cond)?));
            }
            for cond in &c.unlesses {
                conds.push(format!("!({})", callable(cond)?));
            }
            resolved.push((c.kind, target, conds.join(" && ")));
        }
        let body = nested(&resolved, &format!("{ACTION_PREFIX}{action}({args})"));
        text.push_str(&format!("  def {action}({args})\n    __start_action(\"{action}\")\n{body}    message\n  end\n\n"));
    }
    // This class's own block callbacks and lambda conditions.
    for c in &parsed.callbacks {
        for call in std::iter::once(&c.target).chain(&c.ifs).chain(&c.unlesses) {
            if let Call::Body(name, body) = call
                && !generated_bodies.iter().any(|(n, _)| n == name)
            {
                generated_bodies.push((name.clone(), body.clone()));
            }
        }
    }
    if !generated_bodies.is_empty() {
        text.push_str("  private\n\n");
        for (name, _) in &generated_bodies {
            text.push_str(&format!("  def {name}\n    nil\n  end\n\n"));
        }
    }
    text.push_str("end\n");
    let (classes, diags) = super::prism::scope(|| super::library_class::ingest_library_classes(text.as_bytes(), "<mailer_callbacks>"));
    let classes = classes.map_err(|_| "the generated chain did not ingest".to_string())?;
    if !diags.is_empty() {
        return Err("the generated chain did not ingest".into());
    }
    let mut methods: Vec<crate::dialect::MethodDef> = classes.into_iter().flat_map(|c| c.methods).collect();
    for m in methods.iter_mut() {
        if let Some((_, body)) = generated_bodies.iter().find(|(n, _)| n == m.name.as_str()) {
            m.body = body.clone();
        }
        m.enclosing_class = Some(lc.name.0.clone());
    }
    Ok(methods)
}

/// The chain for one action, from `resolved[0]` inward to the action.
fn nested(resolved: &[(Kind, String, String)], action: &str) -> String {
    fn go(rest: &[(Kind, String, String)], action: &str, indent: usize) -> String {
        let pad = " ".repeat(indent);
        let Some(((kind, name, cond), rest)) = rest.split_first() else {
            return format!("{pad}{action}\n");
        };
        let guard = if cond.is_empty() { String::new() } else { format!(" if {cond}") };
        match kind {
            Kind::Before => format!("{pad}{name}{guard}\n{}", go(rest, action, indent)),
            Kind::After => format!("{}{pad}{name}{guard}\n", go(rest, action, indent)),
            Kind::Around if cond.is_empty() => {
                format!("{pad}{name} do\n{}{pad}end\n", go(rest, action, indent + 2))
            }
            Kind::Around => format!(
                "{pad}if {cond}\n{pad}  {name} do\n{}{pad}  end\n{pad}else\n{}{pad}end\n",
                go(rest, action, indent + 4),
                go(rest, action, indent + 2)
            ),
        }
    }
    go(resolved, action, 4)
}
