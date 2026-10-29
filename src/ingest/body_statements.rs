//! A library class or module body statement that only Ruby itself
//! answers, which the Ruby and JRuby trees write back where the source
//! has it ([`crate::expr::BODY_STATEMENT`]):
//!
//! * a loop over a literal that `class_eval`s a heredoc or
//!   `define_method`s per element (activemerchant's `%w(month year).each
//!   do |m| class_eval <<~RUBY … end`, activeresource's `{ true => %w[post
//!   put], … }.each do … module_eval … end`);
//! * a call to a core `Module` method (`freeze`, `self.define_method`);
//! * a write through a class-side writer the class or an app ancestor
//!   defines (`self.require_name = true` below `class << self;
//!   attr_accessor :require_name; end`, and in a subclass);
//! * any of these, or `delegate`, in the class's own `class << self`
//!   ([`crate::expr::SINGLETON_BODY`]) of a module or of a class no
//!   runtime base gives class methods to.
//!
//! A statement qualifies only when every call it makes on the class
//! (receiverless, or on `self`, outside the bodies of the methods it
//! defines) is one of those, and every call on another of the app's
//! classes is to a class method the app defines there: nothing it runs
//! is a framework macro the runtime does not define, which would raise
//! as the class loads. The rest stay dropped and reported by name.

use crate::det::HashSet;

use crate::App;
use crate::dialect::MethodReceiver;
use crate::expr::{Expr, ExprNode, LValue};

/// `Module` / `Class` methods every Ruby class answers, plus `delegate`,
/// which the Ruby family's runtime defines on `Module`
/// (`runtime/spinel/module_delegate.rb`).
const MODULE_METHODS: &[&str] = &[
    "class_eval",
    "module_eval",
    "class_exec",
    "module_exec",
    "define_method",
    "define_singleton_method",
    "attr_reader",
    "attr_writer",
    "attr_accessor",
    "alias_method",
    "freeze",
    "const_set",
    "remove_method",
    "undef_method",
    "instance_variable_set",
    "delegate",
];

/// Calls whose block is the body of a method they define, which runs
/// later with an instance (or the singleton) as `self`.
const DEFINES_FROM_BLOCK: &[&str] = &["define_method", "define_singleton_method"];

/// Superclasses from Ruby's core, whose class side gives a body nothing
/// a runtime base would.
const CORE_CLASSES: &[&str] = &[
    "Object",
    "BasicObject",
    "Struct",
    "Exception",
    "StandardError",
    "RuntimeError",
    "ArgumentError",
    "TypeError",
    "NameError",
    "NotImplementedError",
    "IOError",
    "KeyError",
    "IndexError",
    "RangeError",
    "SecurityError",
];

pub(super) fn mark(app: &mut App) {
    let mut class_side: std::collections::BTreeMap<String, HashSet<String>> = Default::default();
    let mut parents: std::collections::BTreeMap<String, Option<String>> = Default::default();
    for lc in app.library_classes.iter().filter(|lc| lc.origin.is_none()) {
        let name = lc.name.0.as_str().to_string();
        class_side
            .entry(name.clone())
            .or_default()
            .extend(lc.methods.iter().filter(|m| m.receiver == MethodReceiver::Class).map(|m| m.name.as_str().to_string()));
        if !lc.is_module {
            let parent = lc.parent.as_ref().map(|p| p.0.as_str().trim_start_matches("::").to_string());
            let entry = parents.entry(name).or_insert(None);
            if entry.is_none() {
                *entry = parent;
            }
        }
    }
    let models: HashSet<String> = app.models.iter().map(|m| m.name.0.as_str().to_string()).collect();
    let index = Index { class_side: &class_side, parents: &parents, models: &models };
    for lc in app.library_classes.iter_mut().filter(|lc| lc.origin.is_none()) {
        let name = lc.name.0.as_str().to_string();
        let singleton_ok = lc.is_module || index.answered_by_app(&name);
        let constants: HashSet<String> = lc.constants.iter().map(|(n, _)| n.as_str().to_string()).collect();
        for expr in &mut lc.unknown_calls {
            if expr.decisions & crate::expr::REPLAYED_CLASS_BODY != 0 || expr.span.is_synthetic() {
                continue;
            }
            if expr.decisions & crate::expr::SINGLETON_BODY != 0 && !singleton_ok {
                continue;
            }
            let check = Check { owner: &name, index: &index, constants: &constants };
            if check.statement(expr) {
                expr.decisions |= crate::expr::BODY_STATEMENT;
            }
        }
    }
}

struct Index<'a> {
    class_side: &'a std::collections::BTreeMap<String, HashSet<String>>,
    parents: &'a std::collections::BTreeMap<String, Option<String>>,
    models: &'a HashSet<String>,
}

impl Index<'_> {
    /// Whether `name` or an app ancestor of it defines the class method
    /// `method`.
    fn defines(&self, name: &str, method: &str) -> bool {
        let mut seen: Vec<&str> = Vec::new();
        let mut at = Some(name);
        while let Some(class) = at {
            if seen.contains(&class) {
                return false;
            }
            seen.push(class);
            if self.class_side.get(class).is_some_and(|methods| methods.contains(method)) {
                return true;
            }
            at = self.parents.get(class).and_then(|p| p.as_deref());
        }
        false
    }

    /// Whether `name`'s superclasses are the app's own classes up to a
    /// core one: no runtime base answers its class side.
    fn answered_by_app(&self, name: &str) -> bool {
        let mut seen: Vec<&str> = vec![name];
        let mut parent = self.parents.get(name).and_then(|p| p.as_deref());
        while let Some(p) = parent {
            if self.models.contains(p) || seen.contains(&p) {
                return false;
            }
            match self.parents.get(p) {
                Some(next) => {
                    seen.push(p);
                    parent = next.as_deref();
                }
                None => return CORE_CLASSES.contains(&p),
            }
        }
        true
    }
}

struct Check<'a> {
    owner: &'a str,
    index: &'a Index<'a>,
    constants: &'a HashSet<String>,
}

impl Check<'_> {
    fn answered(&self, method: &str) -> bool {
        MODULE_METHODS.contains(&method) || self.index.defines(self.owner, method)
    }

    fn statement(&self, expr: &Expr) -> bool {
        match &*expr.node {
            // `self.require_name = true`, `self.strict ||= :on`.
            ExprNode::Assign { target: LValue::Attr { recv, name }, value } => {
                matches!(&*recv.node, ExprNode::SelfRef) && self.answered(&format!("{}=", name.as_str())) && self.walk(value)
            }
            // A call on the class is one of Ruby's own `Module` methods. One
            // to its own class methods is `app_extension_calls`' (without
            // a block) or the constant-defining replay's, and a hook's
            // block (`included do`) is the concern splice's.
            ExprNode::Send { recv, method, args, block, .. } => match recv {
                None => MODULE_METHODS.contains(&method.as_str()) && self.walk(expr),
                // `self.require_name = true`, through a writer the class or
                // an app ancestor defines.
                Some(r) if matches!(&*r.node, ExprNode::SelfRef) => {
                    let writer = method.as_str().ends_with('=') && block.is_none();
                    (MODULE_METHODS.contains(&method.as_str()) || writer) && self.walk(expr)
                }
                // A loop: a block over an inert receiver.
                Some(r) => {
                    self.inert(r) && args.iter().all(|a| self.walk(a)) && block.as_ref().is_some_and(|b| self.walk(b))
                }
            },
            _ => false,
        }
    }

    /// A literal, a constant of this body, or a core collection method
    /// on one (`%w[a b].each_with_index`).
    fn inert(&self, expr: &Expr) -> bool {
        match &*expr.node {
            ExprNode::Lit { .. } => true,
            ExprNode::Array { elements, .. } => elements.iter().all(|e| self.inert(e)),
            ExprNode::Hash { entries, .. } => entries.iter().all(|(k, v)| self.inert(k) && self.inert(v)),
            ExprNode::Const { path } => {
                let named: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
                match named.as_slice() {
                    [name] => self.constants.contains(*name),
                    [.., name] => named[..named.len() - 1].join("::") == self.owner && self.constants.contains(*name),
                    [] => false,
                }
            }
            ExprNode::Send { recv: Some(r), block: None, .. } => self.inert(r),
            _ => false,
        }
    }

    /// Every call `expr` makes on the class is answered, and every call
    /// on another app class is to a class method it defines.
    fn walk(&self, expr: &Expr) -> bool {
        match &*expr.node {
            ExprNode::Send { recv, method, args, block, .. } => {
                let ok = match recv {
                    None => self.answered(method.as_str()),
                    Some(r) if matches!(&*r.node, ExprNode::SelfRef) => self.answered(method.as_str()),
                    Some(r) => match &*r.node {
                        ExprNode::Const { path } => {
                            let named: Vec<&str> = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect();
                            let name = named.join("::");
                            if self.index.class_side.contains_key(&name) {
                                self.index.defines(&name, method.as_str())
                            } else {
                                // A constant of this body (`WRITERS.each`).
                                self.inert(r)
                            }
                        }
                        _ => self.walk(r),
                    },
                };
                if !ok || !args.iter().all(|a| self.walk(a)) {
                    return false;
                }
                // A block that is the body of a method runs later, not here.
                let method_body = recv.as_ref().is_none_or(|r| matches!(&*r.node, ExprNode::SelfRef))
                    && DEFINES_FROM_BLOCK.contains(&method.as_str());
                method_body || block.as_ref().is_none_or(|b| self.walk(b))
            }
            ExprNode::Lambda { body, .. } => self.walk(body),
            _ => {
                let mut ok = true;
                expr.node.for_each_child(&mut |child| {
                    if ok && !self.walk(child) {
                        ok = false;
                    }
                });
                ok
            }
        }
    }
}
