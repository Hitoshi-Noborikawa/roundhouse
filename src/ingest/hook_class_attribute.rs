//! ActiveSupport's `class_attribute` and `cattr_*` / `mattr_*` in a
//! library class: in its own body, or in an app module's
//! `self.included(base)` hook on the class that includes it.
//!
//! activemerchant 1.137.0's `PostsData` is the hook shape:
//!
//! ```ruby
//! def self.included(base)
//!   base.class_attribute :ssl_strict
//!   base.ssl_strict = true
//! ```
//!
//! The emitted tree defines no `Class#class_attribute` (a runtime method
//! that defines methods from computed names is one the inference engine
//! could not resolve), so the hook raised NoMethodError at the include.
//! Each declaration becomes plain defs on the class that declares it,
//! or on each app class that includes the module directly:
//!
//! - `self.name`: this class's value once written, else its
//!   superclass's (`@name__written`, as `class_attribute::written_flag`
//!   spells it for controllers): a subclass reads its parent's value,
//!   live, until it writes its own;
//! - `self.name=(value)`: writes this class's own;
//! - `self.name?` (`instance_predicate`): `!!name`;
//! - `name` (`instance_reader`): the instance's own once written, else
//!   `self.class.name`; `name=` (`instance_writer`); `name?`.
//!
//! The declaration itself becomes its seed: in a class body the class
//! ivars set to `default`, in a hook `base.name = default`. Later names
//! of one declaration share the first's value, as ActiveSupport stores
//! one default object under every name.
//!
//! `base.cattr_accessor :pem_file` (activemerchant's `PaypalCommonAPI`)
//! defines `@@pem_file` readers and writers on `base` and its instances,
//! and seeds `@@pem_file` there: with a default, always; without one,
//! nil unless a class variable the includer sees already holds it.
//!
//! A hook may ask first: `if base.respond_to?(:class_attribute)` with the
//! declarations, else a Rails 2 `class_inheritable_accessor`
//! (activemerchant's `PaypalExpressCommon`). Natively that is true, and
//! this expansion is what answers `class_attribute` here, so such an `if`
//! whose branch holds only declarations is that branch.
//!
//! All or nothing per hook: one with a declaration this does not read (a
//! computed name or flag, a block, an unknown option, a call nested in
//! other code) keeps its text, as does a hook no app class includes.

use crate::App;
use crate::dialect::{LibraryClass, MethodDef, MethodReceiver};
use crate::expr::{Expr, ExprNode, LValue, Literal};
use crate::ident::{Symbol, VarId};
use crate::span::Span;

/// One `class_attribute` or `cattr_*` / `mattr_*` declaration.
struct Decl {
    names: Vec<Symbol>,
    kind: Kind,
    /// `default:`, evaluated once and shared by every name.
    default: Option<Expr>,
    span: Span,
}

#[derive(Clone, Copy)]
enum Kind {
    ClassAttribute { instance_reader: bool, instance_writer: bool, instance_predicate: bool },
    ClassVariable { reader: bool, writer: bool, instance_reader: bool, instance_writer: bool },
}

pub(super) fn expand(app: &mut App) {
    expand_class_bodies(app);
    expand_hooks(app);
}

/// `class_attribute :live` written in a library class's own body.
fn expand_class_bodies(app: &mut App) {
    for lc in app.library_classes.iter_mut().filter(|lc| lc.origin.is_none() && !lc.is_module) {
        let mut decls = Vec::new();
        lc.unknown_calls.retain(|call| match declaration(call, None) {
            Some(decl @ Decl { kind: Kind::ClassAttribute { .. }, .. }) => {
                decls.push(decl);
                false
            }
            _ => true,
        });
        let mut added = Vec::new();
        for decl in &decls {
            added.extend(methods(lc, &added, decl, decl.span));
            lc.class_ivar_initializers.extend(class_body_seeds(decl));
        }
        lc.methods.splice(0..0, added);
    }
}

fn expand_hooks(app: &mut App) {
    let names: Vec<String> = app.library_classes.iter().map(|lc| lc.name.0.as_str().to_string()).collect();
    for mi in 0..app.library_classes.len() {
        let module = &app.library_classes[mi];
        if !module.is_module || module.origin.is_some() {
            continue;
        }
        let Some(hi) = module.methods.iter().position(|m| m.receiver == MethodReceiver::Class && m.name.as_str() == "included")
        else {
            continue;
        };
        let Some((base, statements)) = hook_declarations(&module.methods[hi]) else { continue };
        if statements.iter().all(|s| matches!(s, Statement::Other(_))) {
            continue;
        }
        let module_name = module.name.0.as_str().to_string();
        let includers: Vec<usize> = app
            .library_classes
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_module && c.origin.is_none())
            .filter(|(_, c)| {
                c.includes.iter().any(|inc| resolve(&names, inc.0.as_str(), c.name.0.as_str()).as_deref() == Some(module_name.as_str()))
            })
            .map(|(i, _)| i)
            .collect();
        if includers.is_empty() {
            continue;
        }
        for &i in &includers {
            // At the include that runs the hook, which the Ruby trees
            // write after them (`emit::ruby::library::hooked_include_position`).
            let lc = &app.library_classes[i];
            let at = lc
                .includes
                .iter()
                .zip(&lc.include_spans)
                .find(|(inc, _)| resolve(&names, inc.0.as_str(), lc.name.0.as_str()).as_deref() == Some(module_name.as_str()))
                .map(|(_, span)| *span)
                .unwrap_or_else(Span::synthetic);
            let mut added = Vec::new();
            for statement in &statements {
                if let Statement::Declarations(decls) = statement {
                    for decl in decls {
                        added.extend(methods(lc, &added, decl, at));
                    }
                }
            }
            app.library_classes[i].methods.splice(0..0, added);
        }
        let hook = &mut app.library_classes[mi].methods[hi];
        let span = hook.body.span;
        let base = Expr::new(span, ExprNode::Var { id: VarId(0), name: base });
        let exprs = statements
            .into_iter()
            .flat_map(|statement| match statement {
                Statement::Other(expr) => vec![expr],
                Statement::Declarations(decls) => decls.iter().flat_map(|decl| hook_seeds(&base, decl)).collect(),
            })
            .collect();
        hook.body = Expr::new(span, ExprNode::Seq { exprs });
    }
}

enum Statement {
    Declarations(Vec<Decl>),
    Other(Expr),
}

/// The hook's parameter and its statements, each declaration read.
/// `None` when the hook takes anything but one plain parameter, or has
/// a declaration this does not read.
fn hook_declarations(hook: &MethodDef) -> Option<(Symbol, Vec<Statement>)> {
    let [base] = hook.params.as_slice() else { return None };
    if base.keyword || base.rest || base.forwarding || base.default.is_some() || hook.block_param.is_some() {
        return None;
    }
    let base = base.name.clone();
    let body: Vec<Expr> = match &*hook.body.node {
        ExprNode::Seq { exprs } => exprs.clone(),
        _ => vec![hook.body.clone()],
    };
    let mut read = 0;
    let mut unread_branch = 0;
    let mut statements = Vec::new();
    for statement in body {
        if let Some(decl) = declaration(&statement, Some(&base)) {
            read += 1;
            statements.push(Statement::Declarations(vec![decl]));
        } else if let Some((decls, else_count)) = guarded(&statement, &base) {
            read += decls.len();
            unread_branch += else_count;
            statements.push(Statement::Declarations(decls));
        } else {
            statements.push(Statement::Other(statement));
        }
    }
    (declaration_calls(&hook.body) == read + unread_branch).then_some((base, statements))
}

/// `if base.respond_to?(:class_attribute)` whose branch is only
/// `class_attribute` declarations: the declarations, and how many
/// declaration calls the else branch (which does not run) holds.
fn guarded(statement: &Expr, base: &Symbol) -> Option<(Vec<Decl>, usize)> {
    let ExprNode::If { cond, then_branch, else_branch } = &*statement.node else { return None };
    let ExprNode::Send { recv: Some(recv), method, args, block: None, .. } = &*cond.node else { return None };
    if method.as_str() != "respond_to?" || !is_var(recv, base) {
        return None;
    }
    let [ExprNode::Lit { value: Literal::Sym { value } }] = args.iter().map(|a| &*a.node).collect::<Vec<_>>()[..] else {
        return None;
    };
    if value.as_str() != "class_attribute" {
        return None;
    }
    let branch: Vec<&Expr> = match &*then_branch.node {
        ExprNode::Seq { exprs } => exprs.iter().collect(),
        _ => vec![then_branch],
    };
    let decls: Vec<Decl> = branch
        .iter()
        .map(|e| declaration(e, Some(base)).filter(|d| matches!(d.kind, Kind::ClassAttribute { .. })))
        .collect::<Option<_>>()?;
    (!decls.is_empty()).then(|| (decls, declaration_calls(else_branch)))
}

const DECLARATIONS: &[&str] =
    &["class_attribute", "cattr_accessor", "cattr_reader", "cattr_writer", "mattr_accessor", "mattr_reader", "mattr_writer"];

/// How many declaration calls `e` holds, anywhere in it.
fn declaration_calls(e: &Expr) -> usize {
    let mut n = 0;
    fn walk(e: &Expr, n: &mut usize) {
        if let ExprNode::Send { method, .. } = &*e.node
            && DECLARATIONS.contains(&method.as_str())
        {
            *n += 1;
        }
        e.node.for_each_child(&mut |c| walk(c, n));
    }
    walk(e, &mut n);
    n
}

fn is_var(e: &Expr, name: &Symbol) -> bool {
    matches!(&*e.node, ExprNode::Var { name: n, .. } if n == name)
}

/// A declaration on the class (`base` in a hook; no receiver or `self`
/// in a class body) with literal names and literal flags.
fn declaration(call: &Expr, base: Option<&Symbol>) -> Option<Decl> {
    let ExprNode::Send { recv, method, args, block: None, .. } = &*call.node else { return None };
    let on_class = match (recv, base) {
        (Some(r), Some(base)) => is_var(r, base),
        (None, None) => true,
        (Some(r), None) => matches!(&*r.node, ExprNode::SelfRef),
        (None, Some(_)) => false,
    };
    if !on_class || !DECLARATIONS.contains(&method.as_str()) {
        return None;
    }
    let mut names = Vec::new();
    let mut options: Vec<(String, Expr)> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } if options.is_empty() && is_method_name(value.as_str()) => {
                names.push(value.clone())
            }
            ExprNode::Hash { entries, .. } if i + 1 == args.len() => {
                for (k, v) in entries {
                    let ExprNode::Lit { value: Literal::Sym { value } } = &*k.node else { return None };
                    options.push((value.as_str().to_string(), v.clone()));
                }
            }
            _ => return None,
        }
    }
    if names.is_empty() {
        return None;
    }
    let flag = |name: &str| -> Option<Option<bool>> {
        match options.iter().find(|(k, _)| k == name) {
            None => Some(None),
            Some((_, v)) => match &*v.node {
                ExprNode::Lit { value: Literal::Bool { value } } => Some(Some(*value)),
                ExprNode::Lit { value: Literal::Nil } => Some(Some(false)),
                _ => None,
            },
        }
    };
    let default = options.iter().find(|(k, _)| k == "default").map(|(_, v)| v.clone());
    let method = method.as_str();
    let kind = if method == "class_attribute" {
        let known = ["instance_accessor", "instance_reader", "instance_writer", "instance_predicate", "default"];
        if options.iter().any(|(k, _)| !known.contains(&k.as_str())) {
            return None;
        }
        let accessor = flag("instance_accessor")?.unwrap_or(true);
        Kind::ClassAttribute {
            instance_reader: flag("instance_reader")?.unwrap_or(accessor),
            instance_writer: flag("instance_writer")?.unwrap_or(accessor),
            instance_predicate: flag("instance_predicate")?.unwrap_or(true),
        }
    } else {
        let known = ["instance_accessor", "instance_reader", "instance_writer", "default"];
        if options.iter().any(|(k, _)| !known.contains(&k.as_str())) {
            return None;
        }
        let accessor = flag("instance_accessor")?.unwrap_or(true);
        let reader = !method.ends_with("_writer");
        let writer = !method.ends_with("_reader");
        Kind::ClassVariable {
            reader,
            writer,
            instance_reader: reader && flag("instance_reader")?.unwrap_or(accessor),
            instance_writer: writer && flag("instance_writer")?.unwrap_or(accessor),
        }
    };
    Some(Decl { names, kind, default, span: call.span })
}

fn is_method_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_ascii_lowercase()) && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// The class's flag that it wrote its own value of `name`.
fn written(name: &str) -> String {
    super::class_attribute::written_flag(&Symbol::from(name)).as_str().to_string()
}

/// The declaration's defs for `lc`, placed at `at`; a method the class
/// defines itself (or `added` already holds) stays its own.
fn methods(lc: &LibraryClass, added: &[MethodDef], decl: &Decl, at: Span) -> Vec<MethodDef> {
    let mut src = String::from("class RoundhouseClassAttribute\n");
    for name in &decl.names {
        let n = name.as_str();
        let w = written(n);
        match decl.kind {
            Kind::ClassAttribute { instance_reader, instance_writer, instance_predicate } => {
                src.push_str(&format!("  def self.{n}\n    @{w} ? @{n} : superclass.{n}\n  end\n"));
                src.push_str(&format!("  def self.{n}=(value)\n    @{w} = true\n    @{n} = value\n  end\n"));
                if instance_predicate {
                    src.push_str(&format!("  def self.{n}?\n    !!{n}\n  end\n"));
                }
                if instance_reader {
                    src.push_str(&format!("  def {n}\n    @{w} ? @{n} : self.class.{n}\n  end\n"));
                }
                if instance_writer {
                    src.push_str(&format!("  def {n}=(value)\n    @{w} = true\n    @{n} = value\n  end\n"));
                }
                if instance_predicate && instance_reader {
                    src.push_str(&format!("  def {n}?\n    !!{n}\n  end\n"));
                }
            }
            Kind::ClassVariable { reader, writer, instance_reader, instance_writer } => {
                if reader {
                    src.push_str(&format!("  def self.{n}\n    @@{n}\n  end\n"));
                }
                if writer {
                    src.push_str(&format!("  def self.{n}=(value)\n    @@{n} = value\n  end\n"));
                }
                if instance_reader {
                    src.push_str(&format!("  def {n}\n    @@{n}\n  end\n"));
                }
                if instance_writer {
                    src.push_str(&format!("  def {n}=(value)\n    @@{n} = value\n  end\n"));
                }
            }
        }
    }
    src.push_str("end\n");
    let (parsed, _) = super::prism::scope(|| super::ingest_library_classes(src.as_bytes(), "<class_attribute>"));
    let classes = match parsed {
        Ok(classes) => classes,
        Err(err) => {
            super::survey::record(&err);
            return Vec::new();
        }
    };
    let mut out: Vec<MethodDef> = Vec::new();
    for mut method in classes.into_iter().flat_map(|c| c.methods) {
        if lc.methods.iter().chain(added).chain(&out).any(|d| d.name == method.name && d.receiver == method.receiver) {
            continue;
        }
        method.name_span = at;
        out.push(method);
    }
    out
}

/// `@name__written = true; @name = default` per name, where a class body
/// declared it.
fn class_body_seeds(decl: &Decl) -> Vec<Expr> {
    let span = decl.span;
    let ivar = |name: String, value: Expr| {
        Expr::new(span, ExprNode::Assign { target: LValue::Ivar { name: Symbol::from(name) }, value })
    };
    let mut out = Vec::new();
    for (i, name) in decl.names.iter().enumerate() {
        let value = match (&decl.default, i) {
            (Some(_), 1..) => Expr::new(span, ExprNode::Ivar { name: decl.names[0].clone() }),
            (Some(default), 0) => default.clone(),
            (None, _) => Expr::new(span, ExprNode::Lit { value: Literal::Nil }),
        };
        out.push(ivar(written(name.as_str()), Expr::new(span, ExprNode::Lit { value: Literal::Bool { value: true } })));
        out.push(ivar(name.as_str().to_string(), value));
    }
    out
}

/// What the hook's declaration does to `base` once its defs exist.
fn hook_seeds(base: &Expr, decl: &Decl) -> Vec<Expr> {
    let span = decl.span;
    let send = |method: &str, args: Vec<Expr>| {
        let parenthesized = !method.ends_with('=') && !args.is_empty();
        Expr::new(span, ExprNode::Send { recv: Some(base.clone()), method: Symbol::from(method), args, block: None, parenthesized })
    };
    let sym = |name: String| Expr::new(span, ExprNode::Lit { value: Literal::Sym { value: Symbol::from(name) } });
    let nil = || Expr::new(span, ExprNode::Lit { value: Literal::Nil });
    let mut out = Vec::new();
    for (i, name) in decl.names.iter().enumerate() {
        let n = name.as_str();
        match decl.kind {
            // `base.a = default`, `base.b = base.a`.
            Kind::ClassAttribute { .. } => {
                let value = match (&decl.default, i) {
                    (Some(_), 1..) => send(decl.names[0].as_str(), vec![]),
                    (Some(default), 0) => default.clone(),
                    (None, _) => nil(),
                };
                out.push(send(&format!("{n}="), vec![value]));
            }
            // `base.class_variable_set(:@@a, default)`, or nil unless set.
            Kind::ClassVariable { .. } => {
                let cvar = format!("@@{n}");
                match (&decl.default, i) {
                    (Some(_), 1..) => {
                        let first = send("class_variable_get", vec![sym(format!("@@{}", decl.names[0].as_str()))]);
                        out.push(send("class_variable_set", vec![sym(cvar), first]));
                    }
                    (Some(default), 0) => out.push(send("class_variable_set", vec![sym(cvar), default.clone()])),
                    (None, _) => out.push(Expr::new(
                        span,
                        ExprNode::If {
                            cond: send("class_variable_defined?", vec![sym(cvar.clone())]),
                            then_branch: nil(),
                            else_branch: send("class_variable_set", vec![sym(cvar), nil()]),
                        },
                    )),
                }
            }
        }
    }
    out
}

/// `name` as written in `scope`'s body, looked up from its nesting
/// outward (`Redirects` in `Redir::Express` is `Redir::Redirects`).
fn resolve(names: &[String], name: &str, scope: &str) -> Option<String> {
    if let Some(absolute) = name.strip_prefix("::") {
        return names.iter().any(|n| n == absolute).then(|| absolute.to_string());
    }
    let mut prefix: Vec<&str> = scope.split("::").collect();
    loop {
        let candidate = if prefix.is_empty() { name.to_string() } else { format!("{}::{name}", prefix.join("::")) };
        if names.iter().any(|n| *n == candidate) {
            return Some(candidate);
        }
        prefix.pop()?;
    }
}
