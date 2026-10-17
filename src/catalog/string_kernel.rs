//! String kernel shared by analysis and the emitters that already
//! mirror it.
//!
//! One row is `(String, name, arity) → return kind + per-target
//! spelling`. `str_method` reads the
//! return kind. Rust and TypeScript print the spelling instead of
//! matching the Ruby name again. Crystal reads the spelling for the
//! same names; its `length` / `include?` / `start_with?` /
//! `end_with?` renames were not receiver-specific, so the Crystal
//! column applies to every receiver.
//!
//! Analyze types the name at every arity. The ladder this table
//! replaces did not consult arity, and a wrong-arity call staying
//! typed is what keeps it off the error ledger. Emitters bridge only
//! `arity`.

use crate::ty::Ty;

/// Return kind of one kernel method. Only the three shapes this
/// subset actually returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelReturn {
    Int,
    Bool,
    Str,
}

impl KernelReturn {
    pub fn ty(self) -> Ty {
        match self {
            KernelReturn::Int => Ty::Int,
            KernelReturn::Bool => Ty::Bool,
            KernelReturn::Str => Ty::Str,
        }
    }
}

/// Rust bridge. The `&str` is the Rust method; the variant is the
/// shape around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustSpell {
    /// `{recv}.{method}()`
    Call0(&'static str),
    /// `{recv}.{method}(&*({arg}))`. Pattern methods take `&str`, not
    /// `String`, so a computed argument (`ap.start_with?(pp[0, n])`)
    /// needs the reborrow. On a literal `&str`, `&*` is a no-op.
    Call1Borrow(&'static str),
    /// `{recv}.chars().count() as i64`. The cast is non-primary; the
    /// decide pass stamps `NEEDS_PARENS` when this send is a chained
    /// receiver.
    CharCount,
    /// `{recv}.parse::<i64>().unwrap_or(0)`. Ruby `to_i` parses a
    /// leading integer and answers 0 when the rest is not numeric.
    /// `parse::<i64>` requires the whole string, so the failure path
    /// is `unwrap_or(0)`. `&str` and `String` both implement it, which
    /// is why the Symbol arm can share this row.
    ParseI64,
    /// `{recv}.trim().to_string()`. `trim` borrows; Ruby `strip`
    /// returns a new String.
    TrimOwned,
}

/// TypeScript bridge for a receiver the typer has settled as `String`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsSpell {
    /// `{recv}.length === 0`
    LengthEqZero,
    /// `{recv}.{member}`
    Member(&'static str),
    /// `{recv}.{method}()`
    Method0(&'static str),
    /// `{recv}.{method}({arg})`
    Method1(&'static str),
    /// `Number({recv})`
    Number,
}

/// One kernel method.
pub struct StringKernel {
    pub name: &'static str,
    /// Arity the Rust and TypeScript bridges accept.
    pub arity: usize,
    pub returns: KernelReturn,
    pub rust: RustSpell,
    pub typescript: TsSpell,
    /// Crystal method name. Equal to `name` when Crystal spells it
    /// the same way.
    pub crystal: &'static str,
    /// The untyped TypeScript fallback already rewrote this name
    /// (`strip` → `trim`, `include?` → `includes`) before the
    /// receiver was known. Other rows stay on the Ruby spelling
    /// there, which is what that fallback did.
    pub untyped_typescript: bool,
}

/// The kernel Rust and TypeScript already mirrored.
pub const STRING_KERNEL: &[StringKernel] = &[
    StringKernel {
        name: "empty?",
        arity: 0,
        returns: KernelReturn::Bool,
        rust: RustSpell::Call0("is_empty"),
        typescript: TsSpell::LengthEqZero,
        crystal: "empty?",
        untyped_typescript: false,
    },
    StringKernel {
        name: "length",
        arity: 0,
        returns: KernelReturn::Int,
        rust: RustSpell::CharCount,
        typescript: TsSpell::Member("length"),
        crystal: "size",
        untyped_typescript: false,
    },
    StringKernel {
        name: "size",
        arity: 0,
        returns: KernelReturn::Int,
        rust: RustSpell::CharCount,
        typescript: TsSpell::Member("length"),
        crystal: "size",
        untyped_typescript: false,
    },
    StringKernel {
        name: "upcase",
        arity: 0,
        returns: KernelReturn::Str,
        rust: RustSpell::Call0("to_uppercase"),
        typescript: TsSpell::Method0("toUpperCase"),
        crystal: "upcase",
        untyped_typescript: false,
    },
    StringKernel {
        name: "downcase",
        arity: 0,
        returns: KernelReturn::Str,
        rust: RustSpell::Call0("to_lowercase"),
        typescript: TsSpell::Method0("toLowerCase"),
        crystal: "downcase",
        untyped_typescript: false,
    },
    StringKernel {
        name: "strip",
        arity: 0,
        returns: KernelReturn::Str,
        rust: RustSpell::TrimOwned,
        typescript: TsSpell::Method0("trim"),
        crystal: "strip",
        untyped_typescript: true,
    },
    StringKernel {
        name: "start_with?",
        arity: 1,
        returns: KernelReturn::Bool,
        rust: RustSpell::Call1Borrow("starts_with"),
        typescript: TsSpell::Method1("startsWith"),
        crystal: "starts_with?",
        untyped_typescript: false,
    },
    StringKernel {
        name: "end_with?",
        arity: 1,
        returns: KernelReturn::Bool,
        rust: RustSpell::Call1Borrow("ends_with"),
        typescript: TsSpell::Method1("endsWith"),
        crystal: "ends_with?",
        untyped_typescript: false,
    },
    StringKernel {
        name: "include?",
        arity: 1,
        returns: KernelReturn::Bool,
        rust: RustSpell::Call1Borrow("contains"),
        typescript: TsSpell::Method1("includes"),
        crystal: "includes?",
        untyped_typescript: true,
    },
    StringKernel {
        name: "to_i",
        arity: 0,
        returns: KernelReturn::Int,
        rust: RustSpell::ParseI64,
        typescript: TsSpell::Number,
        crystal: "to_i",
        untyped_typescript: false,
    },
];

/// The row for `name` at any arity. Analysis uses this.
pub fn by_name(name: &str) -> Option<&'static StringKernel> {
    STRING_KERNEL.iter().find(|entry| entry.name == name)
}

/// The row whose bridge accepts `arity`. Emitters use this.
pub fn lookup(name: &str, arity: usize) -> Option<&'static StringKernel> {
    STRING_KERNEL
        .iter()
        .find(|entry| entry.name == name && entry.arity == arity)
}

/// Return type for a kernel name, ignoring arity.
pub fn return_ty(name: &str) -> Option<Ty> {
    by_name(name).map(|entry| entry.returns.ty())
}

/// Crystal spelling of a kernel name, for every receiver. `None` when
/// `name` is not in the kernel.
pub fn crystal_name(name: &str) -> Option<&'static str> {
    by_name(name).map(|entry| entry.crystal)
}

/// Rust source for one bridged call. `args` is the already-emitted
/// argument list; [`RustSpell::Call1Borrow`] reads the first.
pub fn render_rust(entry: &StringKernel, recv: &str, args: &[String]) -> String {
    match entry.rust {
        RustSpell::Call0(method) => format!("{recv}.{method}()"),
        RustSpell::Call1Borrow(method) => format!("{recv}.{method}(&*({}))", args[0]),
        RustSpell::CharCount => format!("{recv}.chars().count() as i64"),
        RustSpell::ParseI64 => format!("{recv}.parse::<i64>().unwrap_or(0)"),
        RustSpell::TrimOwned => format!("{recv}.trim().to_string()"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::str_method;
    use crate::diagnostic::Severity;
    use crate::emit::crystal;
    use crate::emit::rust::dispatch_method_by_recv_ty;

    fn rust_bridge(recv: &Expr, method: &str, args: &[Expr]) -> Option<String> {
        crate::emit::rust::expr::with_emit_ctx(crate::emit::rust::EmitCtx::default(), || {
            dispatch_method_by_recv_ty(recv, method, args)
        })
    }
    use crate::emit::typescript;
    use crate::expr::{Expr, ExprNode, Literal};
    use crate::ident::Symbol;
    use crate::span::Span;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Oracle for one call on the literal `"ab"` (and `"a"` when the
    /// row takes an argument). These are the spellings the bridges
    /// emitted before the table existed. A row whose emitter diverges
    /// fails here.
    fn emitted(name: &str) -> (&'static str, &'static str, &'static str) {
        match name {
            "empty?" => (
                r#""ab".is_empty()"#,
                r#""ab".length === 0"#,
                r#""ab".empty?"#,
            ),
            "length" | "size" => (
                r#""ab".chars().count() as i64"#,
                r#""ab".length"#,
                r#""ab".size.to_i64"#,
            ),
            "upcase" => (
                r#""ab".to_uppercase()"#,
                r#""ab".toUpperCase()"#,
                r#""ab".upcase"#,
            ),
            "downcase" => (
                r#""ab".to_lowercase()"#,
                r#""ab".toLowerCase()"#,
                r#""ab".downcase"#,
            ),
            "strip" => (
                r#""ab".trim().to_string()"#,
                r#""ab".trim()"#,
                r#""ab".strip"#,
            ),
            "start_with?" => (
                r#""ab".starts_with(&*("a"))"#,
                r#""ab".startsWith("a")"#,
                r#""ab".starts_with?("a")"#,
            ),
            "end_with?" => (
                r#""ab".ends_with(&*("a"))"#,
                r#""ab".endsWith("a")"#,
                r#""ab".ends_with?("a")"#,
            ),
            "include?" => (
                r#""ab".contains(&*("a"))"#,
                r#""ab".includes("a")"#,
                r#""ab".includes?("a")"#,
            ),
            "to_i" => (
                r#""ab".parse::<i64>().unwrap_or(0)"#,
                r#"Number("ab")"#,
                r#""ab".to_i64"#,
            ),
            other => panic!("no emit oracle for `{other}`"),
        }
    }

    fn call_source(entry: &StringKernel) -> String {
        if entry.arity == 0 {
            format!("\"ab\".{}", entry.name)
        } else {
            format!("\"ab\".{}(\"a\")", entry.name)
        }
    }

    fn analyzed_sends() -> Vec<(String, Expr)> {
        let mut body = String::from("class Probe\n");
        for (i, entry) in STRING_KERNEL.iter().enumerate() {
            body.push_str(&format!("  def m{i}\n    {}\n  end\n", call_source(entry)));
        }
        body.push_str("end\n");
        let mut app = crate::ingest::ingest_app_from_tree(HashMap::from([(
            PathBuf::from("app/lib/probe.rb"),
            body.into_bytes(),
        )]))
        .unwrap();
        crate::analyze::Analyzer::new(&app).analyze(&mut app);
        let errors: Vec<_> = crate::analyze::diagnose(&app)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        let class = app
            .library_classes
            .iter()
            .find(|class| class.name.0.as_str() == "Probe")
            .expect("Probe");
        STRING_KERNEL
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let method = class
                    .methods
                    .iter()
                    .find(|m| m.name.as_str() == format!("m{i}"))
                    .unwrap_or_else(|| panic!("missing m{i}"));
                (entry.name.to_string(), find_send(&method.body, entry.name))
            })
            .collect()
    }

    fn find_send(expr: &Expr, method: &str) -> Expr {
        let mut found = None;
        let mut pending = vec![expr];
        while let Some(current) = pending.pop() {
            if let ExprNode::Send { method: name, .. } = &*current.node {
                if name.as_str() == method {
                    found = Some(current.clone());
                }
            }
            current
                .node
                .for_each_child(&mut |child| pending.push(child));
        }
        found.unwrap_or_else(|| panic!("no `{method}` send"))
    }

    #[test]
    fn analyze_rust_and_typescript_match_the_string_kernel() {
        let sends = analyzed_sends();
        assert_eq!(sends.len(), STRING_KERNEL.len());
        for entry in STRING_KERNEL {
            assert_eq!(
                str_method(&Symbol::from(entry.name)),
                entry.returns.ty(),
                "analyze dropped `{}`",
                entry.name
            );
            let send = sends
                .iter()
                .find(|(name, _)| name == entry.name)
                .unwrap_or_else(|| panic!("missing analyzed {}", entry.name))
                .1
                .clone();
            assert_eq!(send.ty, Some(entry.returns.ty()), "{}", entry.name);
            let ExprNode::Send {
                recv, method, args, ..
            } = &*send.node
            else {
                panic!("not a send");
            };
            let recv = recv.as_ref().expect("receiver");
            assert_eq!(recv.ty, Some(Ty::Str), "{}", entry.name);
            let rust = rust_bridge(recv, method.as_str(), args)
                .unwrap_or_else(|| panic!("rust dropped {}", entry.name));
            let ts = typescript::emit_expr_for_runtime(&send);
            let cr = crystal::emit_expr_for_runtime(&send);
            let (want_rust, want_ts, want_crystal) = emitted(entry.name);
            assert_eq!(rust, want_rust, "rust {}", entry.name);
            assert_eq!(ts, want_ts, "typescript {}", entry.name);
            assert_eq!(cr, want_crystal, "crystal {}", entry.name);
        }
    }

    /// A Symbol receiver shares the Rust string bridge. The table is
    /// keyed by String; the shared arm still prints its spelling.
    #[test]
    fn rust_symbol_receiver_uses_the_string_kernel_spelling() {
        let mut recv = Expr::new(
            Span::synthetic(),
            ExprNode::Lit {
                value: Literal::Sym {
                    value: Symbol::from("ab"),
                },
            },
        );
        recv.ty = Some(Ty::Sym);
        let rust = rust_bridge(&recv, "empty?", &[]).expect("symbol empty? bridge");
        assert_eq!(rust, r#""ab".is_empty()"#);
    }

    /// Wrong arity stays typed, and the Rust bridge declines it.
    #[test]
    fn wrong_arity_stays_typed_and_unbridged() {
        assert_eq!(str_method(&Symbol::from("upcase")), Ty::Str);
        assert!(lookup("upcase", 1).is_none());
        let mut recv = Expr::new(
            Span::synthetic(),
            ExprNode::Lit {
                value: Literal::Str { value: "ab".into() },
            },
        );
        recv.ty = Some(Ty::Str);
        let arg = Expr::new(
            Span::synthetic(),
            ExprNode::Lit {
                value: Literal::Int { value: 1 },
            },
        );
        assert!(rust_bridge(&recv, "upcase", &[arg]).is_none());
    }
}
