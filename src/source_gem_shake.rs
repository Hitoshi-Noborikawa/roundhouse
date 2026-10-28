//! The part of a gem compiled from source (`ingest::auto_source_gems`)
//! that the program reaches, for the Spinel tree.
//!
//! Spinel compiles every class and method a tree carries, so a gem's code
//! the app never runs still has to compile there. parslet's `Context`
//! defines methods on each instance (`singleton_class.send(:define_method,
//! …)`), a shape spinel refuses, and lobsters never reaches it: only
//! `Transform#apply` builds one, and lobsters parses without transforming.
//!
//! Reached is decided on the analyzed IR, from the app's own code:
//!
//! - a class is reached when a reached body names it (a constant), or it
//!   is the parent, an included module or an enclosing namespace of a
//!   reached class;
//! - a call reaches the method the receiver's class answers it with: the
//!   named class's class method for `Const.m`, the receiver's type for a
//!   typed receiver, and `self`'s class and its reached descendants for a
//!   receiverless call. A receiver of unknown type reaches the method of
//!   that name on every reached class, so a dispatch the analysis cannot
//!   pin keeps too much rather than too little; but a class nothing names
//!   (`Transform`) is not one of them.
//!
//! A gem class nothing reaches is left out; a reached one keeps its
//! reached methods, its accessors, and the methods Ruby calls without a
//! call site (`initialize`, `to_s`, `==`, …).

use crate::App;
use crate::det::{HashMap, HashSet};
use crate::dialect::{AccessorKind, LibraryClass, MethodReceiver};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ty::Ty;

/// Methods Ruby dispatches without a textual call site.
const IMPLICIT: &[&str] = &[
    "initialize", "initialize_copy", "initialize_dup", "initialize_clone", "to_s", "to_str", "to_a", "to_ary",
    "to_h", "to_hash", "to_proc", "to_sym", "to_i", "to_int", "to_f", "inspect", "hash", "eql?", "==", "===",
    "<=>", "each", "call", "coerce", "method_missing", "respond_to_missing?", "inherited", "included",
    "extended", "prepended", "method_added", "freeze", "dup", "clone",
];

#[derive(Clone, PartialEq, Eq, Hash)]
enum Target {
    /// A receiver of unknown type.
    Any,
    /// `self`'s or a typed receiver's class (and its reached descendants);
    /// `true` for the class side.
    Virtual(String, bool),
    /// `super` from a method of this class.
    Super(String, bool),
}

type MethodKey = (String, bool, String);

pub(crate) fn shake(app: &App) -> Option<App> {
    if app.auto_source_gems.is_empty() {
        return None;
    }
    let lock = app.gem_lock.as_ref()?;
    let mounted = |file: crate::span::FileId| {
        app.source_path(file).is_some_and(|path| {
            app.auto_source_gems
                .iter()
                .any(|gem| lock.version_of(gem).is_some_and(|v| crate::ingest::source_gems::file_of(path, gem, v)))
        })
    };
    let file_of_class = |c: &LibraryClass| {
        // The class's own header first: a macro the gem defines (parslet's `rule`) leaves its expanded methods' spans in the gem.
        std::iter::once(c.parent_span.file)
            .chain(c.include_spans.iter().map(|span| span.file))
            .chain(c.constants.iter().map(|(_, value)| value.span.file))
            .chain(c.methods.iter().map(|m| m.body.span.file))
            .find(|file| file.0 != 0)
    };
    let is_gem = |c: &LibraryClass| file_of_class(c).is_some_and(mounted);
    let mut gem_names: HashSet<String> = app.library_classes.iter().filter(|c| is_gem(c)).map(|c| c.name.0.to_string()).collect();
    if gem_names.is_empty() {
        return None;
    }
    // Not the app's: a partless reopening (`class Expression` only enclosing `Treetop`) has no file, but its name is the gem's.
    let partless: Vec<String> = app
        .library_classes
        .iter()
        .filter(|c| file_of_class(c).is_none())
        .map(|c| c.name.0.to_string())
        .filter(|n| gem_names.contains(n) || gem_names.iter().any(|g| g.starts_with(&format!("{n}::")) || n.starts_with(&format!("{g}::"))))
        .collect();
    gem_names.extend(partless);
    let is_gem = |c: &LibraryClass| gem_names.contains(c.name.0.as_str());
    let mut entries: HashMap<String, Vec<&LibraryClass>> = HashMap::default();
    for c in &app.library_classes {
        entries.entry(c.name.0.to_string()).or_default().push(c);
    }
    let known = |name: &str| entries.contains_key(name);
    // A name read inside `context` may be any enclosing namespace's (`Atoms::Entity` in `Parslet`).
    let resolve = |context: Option<&str>, name: &str| -> Option<String> {
        let name = name.trim_start_matches("::");
        let mut scope = context.unwrap_or("");
        loop {
            if !scope.is_empty() {
                let candidate = format!("{scope}::{name}");
                if known(&candidate) {
                    return Some(candidate);
                }
            }
            match scope.rsplit_once("::") {
                Some((outer, _)) => scope = outer,
                None if !scope.is_empty() => scope = "",
                None => break,
            }
        }
        known(name).then(|| name.to_string())
    };
    let parent_of = |name: &str| -> Option<String> {
        entries.get(name)?.iter().find_map(|c| c.parent.as_ref()).and_then(|p| resolve(Some(name), p.0.as_str()))
    };
    let includes_of = |name: &str| -> Vec<String> {
        entries
            .get(name)
            .into_iter()
            .flatten()
            .flat_map(|c| c.includes.iter())
            .filter_map(|m| resolve(Some(name), m.0.as_str()))
            .collect()
    };
    // Method resolution order, as Ruby looks a method up: the class, its modules, then its parent's.
    let ancestors = |name: &str| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut current = Some(name.to_string());
        let mut guard = 0;
        while let Some(c) = current {
            guard += 1;
            if guard > 64 || out.contains(&c) {
                break;
            }
            out.push(c.clone());
            let mut stack = includes_of(&c);
            stack.reverse();
            while let Some(m) = stack.pop() {
                if !out.contains(&m) {
                    out.push(m.clone());
                    let mut more = includes_of(&m);
                    more.reverse();
                    stack.extend(more);
                }
            }
            current = parent_of(&c);
        }
        out
    };
    let defines = |owner: &str, class_side: bool, method: &str| {
        entries.get(owner).into_iter().flatten().any(|c| {
            c.methods.iter().any(|m| m.name.as_str() == method && (m.receiver == MethodReceiver::Class) == class_side)
        })
    };

    const DYNAMIC: &str = "\u{0}send ";
    // Walk a body: the calls it makes and the classes it names.
    struct Scan<'a> {
        context: Option<(&'a str, bool, &'a str)>,
        calls: Vec<(Target, String)>,
        consts: Vec<(Option<String>, String)>,
    }
    fn scan(e: &Expr, s: &mut Scan<'_>) {
        match &*e.node {
            ExprNode::Const { path } => {
                let name = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("::");
                s.consts.push((s.context.map(|c| c.0.to_string()), name));
            }
            ExprNode::Send { recv, method, args, .. } => {
                let target = match recv {
                    None => s.context.map_or(Target::Any, |(c, side, _)| Target::Virtual(c.to_string(), side)),
                    Some(r) => match &*r.node {
                        ExprNode::SelfRef => s.context.map_or(Target::Any, |(c, side, _)| Target::Virtual(c.to_string(), side)),
                        ExprNode::Const { path } => {
                            let name = path.iter().map(|p| p.as_str()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("::");
                            Target::Virtual(format!("\u{0}{}\u{0}{name}", s.context.map_or("", |c| c.0)), true)
                        }
                        _ => match r.ty.as_ref().map(Ty::peel_nilable) {
                            Some(Ty::Class { id, .. }) => Target::Virtual(id.0.to_string(), false),
                            _ => Target::Any,
                        },
                    },
                };
                let by_name = matches!(method.as_str(), "send" | "public_send" | "__send__" | "method" | "public_method" | "respond_to?" | "instance_method");
                s.calls.push((target.clone(), method.as_str().to_string()));
                // `send(:name)`: a call by a name held as a Symbol.
                match args.first().map(|a| &*a.node) {
                    Some(ExprNode::Lit { value: Literal::Sym { value } }) if by_name => {
                        s.calls.push((Target::Any, value.as_str().to_string()));
                    }
                    // Not dropped as unknown: svg-graph's `init_with` reaches its option writers by `send(key.to_s + "=", value)`.
                    Some(name) if by_name && matches!(method.as_str(), "send" | "public_send" | "__send__") => {
                        let suffix = match name {
                            ExprNode::Send { method, args, .. } if method.as_str() == "+" => match args.first().map(|a| &*a.node) {
                                Some(ExprNode::Lit { value: Literal::Str { value } }) => value.to_string(),
                                _ => String::new(),
                            },
                            _ => String::new(),
                        };
                        s.calls.push((target, format!("{DYNAMIC}{suffix}")));
                    }
                    _ => {}
                }
            }
            // `method(:name)` / `recv.method(:name)`: ingest's bound-method reference, a call by name later.
            ExprNode::MethodRef { recv, name } => {
                let target = match recv {
                    None => s.context.map_or(Target::Any, |(c, side, _)| Target::Virtual(c.to_string(), side)),
                    Some(r) => match &*r.node {
                        ExprNode::SelfRef => s.context.map_or(Target::Any, |(c, side, _)| Target::Virtual(c.to_string(), side)),
                        _ => Target::Any,
                    },
                };
                s.calls.push((target, name.as_str().to_string()));
            }
            ExprNode::Super { .. } => {
                if let Some((c, side, m)) = s.context {
                    s.calls.push((Target::Super(c.to_string(), side), m.to_string()));
                }
            }
            _ => {}
        }
        e.node.for_each_child(&mut |child| scan(child, s));
    }

    let mut calls: Vec<(Target, String)> = Vec::new();
    let mut consts: Vec<(Option<String>, String)> = Vec::new();
    // Roots: every body outside the gems, and the app's own classes with `self` known.
    crate::lower::for_each_emit_body_ref(app, &mut |e| {
        if e.span.file.0 == 0 || !mounted(e.span.file) {
            let mut s = Scan { context: None, calls: Vec::new(), consts: Vec::new() };
            scan(e, &mut s);
            calls.extend(s.calls);
            consts.extend(s.consts);
        }
    });
    let mut live_classes: HashSet<String> = HashSet::default();
    for c in app.library_classes.iter().filter(|c| !is_gem(c)) {
        live_classes.insert(c.name.0.to_string());
        let name = c.name.0.as_str();
        for m in &c.methods {
            let mut s = Scan { context: Some((name, m.receiver == MethodReceiver::Class, m.name.as_str())), calls: Vec::new(), consts: Vec::new() };
            scan(&m.body, &mut s);
            calls.extend(s.calls);
            consts.extend(s.consts);
        }
    }
    for m in &app.models {
        if let Some(p) = &m.parent {
            consts.push((None, p.0.to_string()));
        }
    }

    let mut live_methods: HashSet<MethodKey> = HashSet::default();
    let mut scanned_methods: HashSet<MethodKey> = HashSet::default();
    let mut scanned_bodies: HashSet<String> = HashSet::default();
    loop {
        let mut changed = false;
        // Classes: named ones, then what loads with a reached one.
        for (context, name) in std::mem::take(&mut consts) {
            if let Some(c) = resolve(context.as_deref(), &name)
                && live_classes.insert(c)
            {
                changed = true;
            }
        }
        loop {
            let mut more: Vec<String> = Vec::new();
            for c in &live_classes {
                more.extend(parent_of(c));
                more.extend(includes_of(c));
                let mut outer = c.as_str();
                while let Some((o, _)) = outer.rsplit_once("::") {
                    if known(o) {
                        more.push(o.to_string());
                    }
                    outer = o;
                }
            }
            let before = live_classes.len();
            live_classes.extend(more);
            if live_classes.len() == before {
                break;
            }
            changed = true;
        }
        // A reached gem class's body runs as it loads, and Ruby may call its implicit methods at any time.
        for c in live_classes.clone() {
            if gem_names.contains(&c) && scanned_bodies.insert(c.clone()) {
                for entry in entries.get(&c).into_iter().flatten() {
                    for m in entry.methods.iter().filter(|m| IMPLICIT.contains(&m.name.as_str())) {
                        live_methods.insert((c.clone(), m.receiver == MethodReceiver::Class, m.name.to_string()));
                    }
                }
                for entry in entries.get(&c).into_iter().flatten() {
                    let mut s = Scan { context: Some((c.as_str(), true, "")), calls: Vec::new(), consts: Vec::new() };
                    for (_, value) in &entry.constants {
                        scan(value, &mut s);
                    }
                    for call in &entry.unknown_calls {
                        scan(call, &mut s);
                    }
                    calls.extend(s.calls);
                    consts.extend(s.consts);
                }
                changed = true;
            }
        }
        // Calls: the method each one reaches on the reached classes.
        let mut reached: Vec<MethodKey> = Vec::new();
        let mut named: Vec<(Target, String)> = Vec::new();
        for (target, method) in &calls {
            let Some(suffix) = method.strip_prefix(DYNAMIC) else { continue };
            let classes: Vec<&String> = match target {
                Target::Virtual(c, _) if !c.starts_with('\u{0}') => live_classes.iter().filter(|d| ancestors(d).contains(c)).collect(),
                _ => live_classes.iter().collect(),
            };
            for c in classes {
                for entry in entries.get(c.as_str()).into_iter().flatten() {
                    for m in entry.methods.iter().filter(|m| m.name.as_str().ends_with(suffix)) {
                        named.push((target.clone(), m.name.to_string()));
                    }
                }
            }
        }
        for (target, method) in calls.iter().filter(|(_, m)| !m.starts_with(DYNAMIC)).chain(named.iter()) {
            let (receivers, class_side): (Vec<String>, bool) = match target {
                Target::Any => {
                    for c in &live_classes {
                        for side in [false, true] {
                            if let Some(owner) = ancestors(c).into_iter().find(|a| defines(a, side, method)) {
                                reached.push((owner, side, method.clone()));
                            }
                        }
                    }
                    continue;
                }
                Target::Super(c, side) => {
                    if let Some(owner) = ancestors(c).into_iter().skip(1).find(|a| defines(a, *side, method)) {
                        reached.push((owner, *side, method.clone()));
                    }
                    continue;
                }
                Target::Virtual(c, side) => {
                    // A constant receiver: resolved where it was written.
                    let c = match c.strip_prefix('\u{0}').and_then(|rest| rest.split_once('\u{0}')) {
                        Some((context, name)) => match resolve((!context.is_empty()).then_some(context), name) {
                            Some(resolved) => resolved,
                            None => continue,
                        },
                        None => c.clone(),
                    };
                    let descendants: Vec<String> =
                        live_classes.iter().filter(|d| ancestors(d).contains(&c)).cloned().chain(std::iter::once(c.clone())).collect();
                    (descendants, *side)
                }
            };
            for d in receivers {
                if let Some(owner) = ancestors(&d).into_iter().find(|a| defines(a, class_side, method)) {
                    reached.push((owner, class_side, method.clone()));
                }
            }
        }
        for key in reached {
            if live_methods.insert(key.clone()) {
                changed = true;
            }
        }
        // A reached gem method's body is reached code.
        for key in live_methods.clone() {
            let (owner, side, name) = &key;
            if !gem_names.contains(owner) || !scanned_methods.insert(key.clone()) {
                continue;
            }
            for entry in entries.get(owner).into_iter().flatten() {
                for m in entry.methods.iter().filter(|m| m.name.as_str() == name && (m.receiver == MethodReceiver::Class) == *side) {
                    let mut s = Scan { context: Some((owner.as_str(), *side, name.as_str())), calls: Vec::new(), consts: Vec::new() };
                    scan(&m.body, &mut s);
                    for p in m.params.iter().filter_map(|p| p.default.as_ref()) {
                        scan(p, &mut s);
                    }
                    calls.extend(s.calls);
                    consts.extend(s.consts);
                }
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }

    let mut trimmed = app.clone();
    let before: usize = trimmed.library_classes.iter().map(|c| c.methods.len() + 1).sum();
    trimmed.library_classes.retain(|c| !is_gem(c) || live_classes.contains(c.name.0.as_str()));
    for c in trimmed.library_classes.iter_mut().filter(|c| gem_names.contains(c.name.0.as_str())) {
        let owner = c.name.0.to_string();
        c.methods.retain(|m| {
            matches!(m.kind, AccessorKind::AttributeReader | AccessorKind::AttributeWriter)
                || IMPLICIT.contains(&m.name.as_str())
                || live_methods.contains(&(owner.clone(), m.receiver == MethodReceiver::Class, m.name.to_string()))
        });
    }
    let after: usize = trimmed.library_classes.iter().map(|c| c.methods.len() + 1).sum();
    (after != before).then_some(trimmed)
}
