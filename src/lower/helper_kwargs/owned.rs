//! Calls a library class makes to its OWN flattened methods.
//!
//! The class-method and instance-method repairs in the parent module
//! find the callee from the receiver: a constant (`Builder.column(…)`)
//! or a value typed as an instance of a named class. Inside the class
//! itself the receiver names nothing:
//!
//! ```ruby
//! class Builder
//!   def field(name, type = nil, null: true, description: nil) = …
//!   def add(name) = field(name, null: false)        # receiverless
//!   def mark(name) = self.field(name, null: false)  # `self.`
//!   def self.report
//!     b = new                                       # typed `instance`
//!     b.field(:a, :string, null: false)
//!   end
//! end
//! ```
//!
//! A receiverless call and `self.` dispatch on the method's own side
//! (instance or class), and a value typed [`Ty::SelfInstance`] is an
//! instance of the class whose body it is in: the analyzer leaves a bare
//! `new` in a class method as `instance` because the class that runs
//! it is only known per copy (`class_body_new` monomorphizes inherited
//! class methods into each subclass, so the OWNER of the body is that
//! class). All three went unrepaired, and the keyword Hash bound to the
//! first flattened slot.
//!
//! The method is looked up the way Ruby looks it up for these
//! receivers: the owner, the modules it includes, then its superclass
//! chain, among the library classes.
//!
//! `self` is the object that RECEIVED the call, which can be an
//! instance of a subclass. When a subclass redefines the method with a
//! slot layout that moves the keywords differently, no one positional
//! spelling is right for every receiver, so the call is refused.

use std::collections::{HashMap, HashSet};

use crate::app::App;
use crate::dialect::MethodReceiver;
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

use super::{Slot, slot_of};

/// What a library class defines, by side, for the owner-relative lookup.
pub(super) struct OwnerIndex {
    parents: HashMap<String, String>,
    includes: HashMap<String, Vec<String>>,
    children: HashMap<String, Vec<String>>,
    /// Every method a library class defines, flattened or not, with its
    /// slots when it is flattened (`None` when it cannot be respelled).
    defined: HashMap<(String, Side, Symbol), Option<Vec<Slot>>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Side {
    Instance,
    Class,
}

impl From<MethodReceiver> for Side {
    fn from(r: MethodReceiver) -> Self {
        match r {
            MethodReceiver::Instance => Side::Instance,
            MethodReceiver::Class => Side::Class,
        }
    }
}

impl OwnerIndex {
    pub(super) fn new(app: &App) -> Self {
        let names: HashSet<String> =
            app.library_classes.iter().map(|c| c.name.0.as_str().to_string()).collect();
        let mut parents = HashMap::new();
        let mut includes = HashMap::new();
        let mut children: HashMap<String, Vec<String>> = HashMap::new();
        let mut defined = HashMap::new();
        for lc in &app.library_classes {
            let name = lc.name.0.as_str().to_string();
            if let Some(parent) = &lc.parent
                && let Some(parent) = resolve_const(&names, &name, parent.0.as_str())
            {
                children.entry(parent.clone()).or_default().push(name.clone());
                parents.insert(name.clone(), parent);
            }
            includes.insert(
                name.clone(),
                lc.includes
                    .iter()
                    .filter_map(|m| resolve_const(&names, &name, m.0.as_str()))
                    .collect(),
            );
            for m in &lc.methods {
                // Ruby uses the last definition.
                defined.insert(
                    (name.clone(), Side::from(m.receiver), m.name.clone()),
                    flattened_slots(m),
                );
            }
        }
        Self { parents, includes, children, defined }
    }

    /// The definition a call on `side` of `class` reaches, in Ruby's
    /// order: the class, the modules it includes (last included first),
    /// then the superclass. `None` when that definition took no
    /// flattened keywords, or no library class defines the name.
    fn lookup(&self, class: &str, side: Side, method: &Symbol) -> Option<&Vec<Slot>> {
        let mut current = class.to_string();
        let mut seen = HashSet::new();
        while seen.insert(current.clone()) {
            if let Some(found) = self.own_or_included(&current, side, method) {
                return found;
            }
            current = self.parents.get(&current)?.clone();
        }
        None
    }

    /// `class`'s own definition, else an included module's.
    fn own_or_included(&self, class: &str, side: Side, method: &Symbol) -> Option<Option<&Vec<Slot>>> {
        if let Some(found) = self.defined.get(&(class.to_string(), side, method.clone())) {
            return Some(found.as_ref());
        }
        if side != Side::Instance {
            return None;
        }
        self.includes.get(class)?.iter().rev().find_map(|m| {
            self.defined
                .get(&(m.clone(), side, method.clone()))
                .map(Option::as_ref)
        })
    }

    /// Every redefinition of `method` below `class` (transitively).
    fn overrides(&self, class: &str, side: Side, method: &Symbol) -> Vec<Option<&Vec<Slot>>> {
        let mut out = Vec::new();
        let mut stack: Vec<String> = self.children.get(class).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        while let Some(c) = stack.pop() {
            if !seen.insert(c.clone()) {
                continue;
            }
            if let Some(found) = self.own_or_included(&c, side, method) {
                out.push(found);
            }
            stack.extend(self.children.get(&c).cloned().unwrap_or_default());
        }
        out
    }
}

/// `name` as written inside `owner`, resolved against the library
/// classes the way a constant reference is: innermost namespace first.
fn resolve_const(names: &HashSet<String>, owner: &str, name: &str) -> Option<String> {
    let name = name.trim_start_matches("::");
    let mut scope: Vec<&str> = owner.split("::").collect();
    loop {
        let candidate = if scope.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", scope.join("::"))
        };
        if names.contains(&candidate) {
            return Some(candidate);
        }
        if scope.pop().is_none() {
            return None;
        }
    }
}

fn flattened_slots(m: &crate::dialect::MethodDef) -> Option<Vec<Slot>> {
    if m.params.iter().any(|p| p.rest || p.keyword || p.forwarding)
        || !m.params.iter().any(|p| p.from_keyword)
    {
        return None;
    }
    Some(m.params.iter().map(slot_of).collect())
}

impl OwnerIndex {
    /// Instance-side flattened slots by `(class, method)`, as the typed
    /// receiver lookup keys them.
    pub(super) fn instance_slots(&self) -> HashMap<(String, Symbol), Vec<Slot>> {
        self.defined
            .iter()
            .filter(|((_, side, _), _)| *side == Side::Instance)
            .filter_map(|((c, _, m), slots)| Some(((c.clone(), m.clone()), slots.clone()?)))
            .collect()
    }
}

/// The receiver side an owner-relative call dispatches on, or `None`
/// for a call this module does not resolve.
fn owner_side(recv: &Option<Expr>, side: Side) -> Option<Side> {
    match recv {
        None => Some(side),
        Some(r) if matches!(&*r.node, ExprNode::SelfRef) => Some(side),
        Some(r) if matches!(r.ty, Some(crate::ty::Ty::SelfInstance)) => Some(Side::Instance),
        Some(_) => None,
    }
}

/// The `**` half of [`super::refuse_flattened_keyword_splats`] for
/// owner-relative calls.
pub(super) fn refuse_library_class_splats(
    app: &mut App,
    index: &OwnerIndex,
    diagnostics: &mut Vec<crate::diagnostic::Diagnostic>,
) {
    fn walk(
        e: &Expr,
        owner: &str,
        side: Side,
        index: &OwnerIndex,
        diagnostics: &mut Vec<crate::diagnostic::Diagnostic>,
    ) {
        e.node.for_each_child(&mut |c| walk(c, owner, side, index, diagnostics));
        let ExprNode::Send { recv, method, args, .. } = &*e.node else { return };
        let Some(side) = owner_side(recv, side) else { return };
        if let Some(slots) = index.lookup(owner, side, method)
            && super::splat_into_flattened(args, slots)
        {
            super::refuse(e.span, super::SPLAT_REFUSAL, diagnostics);
        }
    }
    for lc in &app.library_classes {
        let owner = lc.name.0.as_str();
        for m in &lc.methods {
            walk(&m.body, owner, Side::from(m.receiver), index, diagnostics);
        }
    }
}

/// Repair, or refuse, every owner-relative call in the library classes'
/// method bodies.
pub(super) fn rewrite_library_classes(
    app: &mut App,
    diagnostics: &mut Vec<crate::diagnostic::Diagnostic>,
) {
    let index = OwnerIndex::new(app);
    if !index.defined.values().any(Option::is_some) {
        return;
    }
    for lc in &mut app.library_classes {
        let owner = lc.name.0.as_str().to_string();
        for m in &mut lc.methods {
            let side = Side::from(m.receiver);
            rewrite(&mut m.body, &owner, side, &index, diagnostics);
        }
    }
}

const SEND_METHODS: &[&str] = &["send", "public_send", "__send__"];

fn rewrite(
    e: &mut Expr,
    owner: &str,
    side: Side,
    index: &OwnerIndex,
    diagnostics: &mut Vec<crate::diagnostic::Diagnostic>,
) {
    e.node.for_each_child_mut(&mut |c| rewrite(c, owner, side, index, diagnostics));
    rewrite_node(e, owner, side, index, diagnostics);
}

fn rewrite_node(
    e: &mut Expr,
    owner: &str,
    side: Side,
    index: &OwnerIndex,
    diagnostics: &mut Vec<crate::diagnostic::Diagnostic>,
) {
    let span = e.span;
    let ExprNode::Send { recv, method, args, .. } = &mut *e.node else {
        return;
    };
    let Some(side) = owner_side(recv, side) else { return };
    if SEND_METHODS.contains(&method.as_str()) {
        refuse_send(span, args, |name| index.lookup(owner, side, name), diagnostics);
        return;
    }
    if !super::carries_keywords(args) {
        return;
    }
    let Some(slots) = index.lookup(owner, side, method) else { return };
    let mut respelled = args.clone();
    if let Err(reason) = super::respell(&mut respelled, slots, true) {
        super::refuse(span, reason, diagnostics);
        return;
    }
    for other in index.overrides(owner, side, method) {
        let mut theirs = args.clone();
        let agrees = other.is_some_and(|slots| super::respell(&mut theirs, slots, true).is_ok())
            && theirs == respelled;
        if !agrees {
            super::refuse(
                span,
                "a subclass redefines this method with different keyword slots, so no one positional order binds for every receiver",
                diagnostics,
            );
            return;
        }
    }
    *args = respelled;
}

/// `recv.send(:name, …, k: v)` to a method whose keywords were
/// flattened: the dispatch is dynamic, so the call is not moved; it is
/// refused when the name is a literal naming such a method, which is
/// the one case where it is known to bind wrongly.
pub(super) fn refuse_send<'a>(
    span: crate::span::Span,
    args: &[Expr],
    lookup: impl Fn(&Symbol) -> Option<&'a Vec<Slot>>,
    diagnostics: &mut Vec<crate::diagnostic::Diagnostic>,
) {
    let Some(first) = args.first() else { return };
    let ExprNode::Lit { value: Literal::Sym { value: name } } = &*first.node else {
        return;
    };
    if !super::carries_keywords(&args[1..]) {
        return;
    }
    if lookup(name).is_some() {
        super::refuse(
            span,
            "keywords passed through `send` to a method whose optional keywords are positional slots",
            diagnostics,
        );
    }
}
