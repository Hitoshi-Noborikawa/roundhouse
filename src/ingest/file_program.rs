//! A gem-source file's file-level program (`App::file_programs`): what
//! Ruby runs when it loads the file beyond the declarations the
//! library-class walk carries, in source order.
//!
//! The library-class walk keeps a file's classes and modules and drops
//! the rest of its top level: `Adaptkit::LOADED << :alpha` above
//! `module AlphaAdapter`, the `require_relative`s an entry file loads its
//! parts with. A gem's own code runs those as it loads, in the order its
//! requires reach them. Each top-level statement becomes an item:
//!
//! - a `class`/`module` declaration loads the emitted files of the
//!   classes the walk carried from it, where it stands;
//! - `require` / `require_relative` of a literal names the file it loads;
//! - a constant write before any statement rides with the file's
//!   classes, as it does today;
//! - any other statement is carried as the expression it is, or, when
//!   the expression ingest refuses it, named as not carried.
//!
//! Every gem-source file has a program, one of declarations alone too,
//! so a `require` of any of the gem's files loads its program. Only gem
//! sources (PATH gems and source gems) get one for now: an app's own
//! `lib/` keeps its current treatment until the constructs it holds
//! (`to_prepare`, `on_load`) are carried here too.

use crate::app::{FileProgram, ProgramItem};
use crate::ingest::prism_api::PrismLocation;
use crate::det::HashSet;
use crate::span::Span;
use crate::{ClassId, Symbol};

use super::expr::ingest_expr;
use super::util::{
    class_name_path, constant_id_str, find_all_classes_with_scope, find_all_module_declarations_with_scope,
    module_name_path,
};

/// `file`'s program; `None` when it does not parse as a program.
/// `carried` names the classes and modules the library-class walk
/// carried from it; `list_dir` answers a directory's entries
/// (`(name, is_dir)`) for a glob require.
pub(super) fn file_program(
    source: &[u8],
    file: &str,
    carried: &HashSet<String>,
    list_dir: &dyn Fn(&std::path::Path) -> Vec<(String, bool)>,
) -> Option<FileProgram> {
    let result = super::prism::parse(source, file);
    let root = result.node();
    let program = root.as_program_node()?;
    let span_of = |node: &ruby_prism::Node<'_>| {
        let loc = node.location();
        Span { file: super::sources::file_id(file), start: loc.start_offset() as u32, end: loc.end_offset() as u32 }
    };
    let mut items = Vec::new();
    for stmt in program.statements().body().iter() {
        if stmt.as_class_node().is_some() || stmt.as_module_node().is_some() {
            // In source order, as Ruby evaluates them; an enclosing
            // declaration starts before what it nests, so it loads first.
            let mut found: Vec<(usize, String)> = Vec::new();
            for (scope, c) in find_all_classes_with_scope(&stmt) {
                let Some(p) = class_name_path(&c) else { continue };
                found.push((c.location().start_offset(), scope.into_iter().chain(p).collect::<Vec<_>>().join("::")));
            }
            for (scope, m) in find_all_module_declarations_with_scope(&stmt) {
                let Some(p) = module_name_path(&m) else { continue };
                found.push((m.location().start_offset(), scope.into_iter().chain(p).collect::<Vec<_>>().join("::")));
            }
            found.sort_by_key(|(start, _)| *start);
            let mut names: Vec<ClassId> = Vec::new();
            for (_, name) in found {
                if carried.contains(&name) && !names.iter().any(|n| n.0.as_str() == name) {
                    names.push(ClassId(Symbol::from(name)));
                }
            }
            if !names.is_empty() {
                items.push(ProgramItem::Declared(names));
            }
            // What the bodies require as they load, after them.
            body_requires(&stmt, &span_of, &mut items);
            continue;
        }
        // A constant rides with the file's classes (`carry_file_constants`).
        if stmt.as_constant_write_node().is_some() || stmt.as_constant_path_write_node().is_some() {
            continue;
        }
        if let Some(globbed) = glob_requires(&stmt, file, list_dir) {
            for (feature, relative) in globbed {
                items.push(ProgramItem::Require { span: span_of(&stmt), feature, relative });
            }
            continue;
        }
        if let Some(call) = stmt.as_call_node() {
            let name = constant_id_str(&call.name());
            if call.receiver().is_none() && call.block().is_none() && matches!(name, "require" | "require_relative") {
                let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
                if let [one] = args.as_slice() {
                    if let Some(feature) = one.as_string_node() {
                        items.push(ProgramItem::Require {
                            span: span_of(&stmt),
                            feature: String::from_utf8_lossy(feature.unescaped()).into_owned(),
                            relative: name == "require_relative",
                        });
                        continue;
                    }
                }
                items.push(ProgramItem::NotCarried {
                    span: span_of(&stmt),
                    message: format!("`{name}` of anything but a string literal is not carried; Ruby loads it when the file loads"),
                });
                continue;
            }
        }
        if is_file_return(&stmt) {
            items.push(ProgramItem::NotCarried {
                span: span_of(&stmt),
                message: "a file-level `return` is not carried; Ruby stops loading the file there".to_string(),
            });
            continue;
        }
        if declares_inside(&stmt) {
            items.push(ProgramItem::NotCarried {
                span: span_of(&stmt),
                message: "a file-level statement that declares a class, module or method is not carried; Ruby defines it when the file loads".to_string(),
            });
            continue;
        }
        match ingest_expr(&stmt, file) {
            Ok(expr) => items.push(ProgramItem::Statement(expr)),
            Err(err) => items.push(ProgramItem::NotCarried {
                span: span_of(&stmt),
                message: format!("the ingest cannot carry this file-level statement: {err}"),
            }),
        }
    }
    Some(FileProgram { source: file.to_string(), items })
}

/// `Dir[File.join(__dir__, "x/**/*.rb")].each { |f| require_relative f }`
/// (or `Dir.glob(…)`, `"#{__dir__}/x/*.rb"`, `require f`): the files the
/// glob finds now, in the order Ruby's `Dir[]` gives them (each
/// directory's entries sorted by name, a directory's own files where
/// the directory sorts), each a `require_relative` of its path from the
/// file. `None` for anything else.
fn glob_requires(
    stmt: &ruby_prism::Node<'_>,
    file: &str,
    list_dir: &dyn Fn(&std::path::Path) -> Vec<(String, bool)>,
) -> Option<Vec<(String, bool)>> {
    let call = stmt.as_call_node()?;
    if constant_id_str(&call.name()) != "each" || call.arguments().is_some() {
        return None;
    }
    let glob = call.receiver()?;
    let glob = glob.as_call_node()?;
    let on_dir = glob.receiver().and_then(|r| r.as_constant_read_node()).is_some_and(|r| constant_id_str(&r.name()) == "Dir");
    if !on_dir || !matches!(constant_id_str(&glob.name()), "[]" | "glob") {
        return None;
    }
    let args: Vec<_> = glob.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let [pattern] = args.as_slice() else { return None };
    let pattern = dir_relative_pattern(pattern)?;
    let block = call.block()?.as_block_node()?;
    let var = block
        .parameters()?
        .as_block_parameters_node()?
        .parameters()?
        .requireds()
        .iter()
        .next()?
        .as_required_parameter_node()
        .map(|p| constant_id_str(&p.name()).to_string())?;
    let inner: Vec<ruby_prism::Node<'_>> = block.body()?.as_statements_node()?.body().iter().collect();
    let [only] = inner.as_slice() else { return None };
    let require = only.as_call_node()?;
    if require.receiver().is_some() || !matches!(constant_id_str(&require.name()), "require" | "require_relative") {
        return None;
    }
    let req_args: Vec<_> = require.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let [arg] = req_args.as_slice() else { return None };
    if arg.as_local_variable_read_node().is_none_or(|r| constant_id_str(&r.name()) != var) {
        return None;
    }
    // `<dir>/**/*.rb` or `<dir>/*.rb`, relative to the file's directory.
    let (dir, recursive) = if let Some(dir) = pattern.strip_suffix("/**/*.rb") {
        (dir, true)
    } else {
        (pattern.strip_suffix("/*.rb")?, false)
    };
    let base = std::path::Path::new(file).parent()?;
    let mut found = Vec::new();
    fn walk(
        at: &std::path::Path,
        rel: &str,
        recursive: bool,
        list_dir: &dyn Fn(&std::path::Path) -> Vec<(String, bool)>,
        out: &mut Vec<String>,
    ) {
        let mut entries = list_dir(at);
        entries.sort();
        for (name, is_dir) in entries {
            if name.starts_with('.') {
                continue;
            }
            let path = format!("{rel}/{name}");
            if is_dir {
                if recursive {
                    walk(&at.join(&name), &path, recursive, list_dir, out);
                }
            } else if let Some(stem) = path.strip_suffix(".rb") {
                out.push(stem.to_string());
            }
        }
    }
    walk(&base.join(dir), dir, recursive, list_dir, &mut found);
    Some(found.into_iter().map(|f| (f, true)).collect())
}

/// A glob argument naming a path under the file's own directory:
/// `File.join(__dir__, "x/**/*.rb")` or `"#{__dir__}/x/**/*.rb"`, as
/// `x/**/*.rb`.
fn dir_relative_pattern(node: &ruby_prism::Node<'_>) -> Option<String> {
    let is_dir = |n: &ruby_prism::Node<'_>| {
        n.as_call_node().is_some_and(|c| c.receiver().is_none() && c.arguments().is_none() && constant_id_str(&c.name()) == "__dir__")
    };
    if let Some(join) = node.as_call_node() {
        let on_file = join.receiver().and_then(|r| r.as_constant_read_node()).is_some_and(|r| constant_id_str(&r.name()) == "File");
        if !on_file || constant_id_str(&join.name()) != "join" {
            return None;
        }
        let args: Vec<_> = join.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        let [first, rest @ ..] = args.as_slice() else { return None };
        if !is_dir(first) || rest.is_empty() {
            return None;
        }
        let parts: Option<Vec<String>> =
            rest.iter().map(|n| n.as_string_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned())).collect();
        return Some(parts?.join("/"));
    }
    let interpolated = node.as_interpolated_string_node()?;
    let parts: Vec<ruby_prism::Node<'_>> = interpolated.parts().iter().collect();
    let [head, tail] = parts.as_slice() else { return None };
    let embedded = head.as_embedded_statements_node()?;
    let statements: Vec<ruby_prism::Node<'_>> = embedded.statements()?.body().iter().collect();
    let [only] = statements.as_slice() else { return None };
    if !is_dir(only) {
        return None;
    }
    let tail = String::from_utf8_lossy(tail.as_string_node()?.unescaped()).into_owned();
    tail.strip_prefix('/').map(str::to_string)
}

/// The requires a declaration's bodies run as they load, in source
/// order, nested bodies included: a literal `require "x"` /
/// `require_relative "x"` statement, and a loop over a literal list
/// whose block requires one file per element
/// (`LIST.each { |n| require_relative "adapters/#{n}_adapter" }`, with
/// `LIST = %w[alpha beta].freeze` written in the same body), as one
/// require per element. Each runs after the declaration's own items:
/// the files it loads see the body's constants defined.
fn body_requires(
    decl: &ruby_prism::Node<'_>,
    span_of: &dyn Fn(&ruby_prism::Node<'_>) -> Span,
    out: &mut Vec<ProgramItem>,
) {
    let body = decl
        .as_class_node()
        .and_then(|c| c.body())
        .or_else(|| decl.as_module_node().and_then(|m| m.body()));
    let Some(body) = body else { return };
    let statements: Vec<ruby_prism::Node<'_>> = match body.as_statements_node() {
        Some(s) => s.body().iter().collect(),
        None => vec![body],
    };
    // The body's literal lists, by constant name.
    let mut lists: Vec<(String, Vec<String>)> = Vec::new();
    for stmt in &statements {
        if let Some(write) = stmt.as_constant_write_node() {
            if let Some(values) = literal_list(&write.value()) {
                lists.push((constant_id_str(&write.name()).to_string(), values));
            }
        }
    }
    for stmt in &statements {
        if stmt.as_class_node().is_some() || stmt.as_module_node().is_some() {
            body_requires(stmt, span_of, out);
            continue;
        }
        let Some(call) = stmt.as_call_node() else { continue };
        if let Some((feature, relative)) = literal_require(&call) {
            out.push(ProgramItem::Require { span: span_of(stmt), feature, relative });
            continue;
        }
        // `LIST.each { |n| require_relative "…#{n}…" }`
        if constant_id_str(&call.name()) != "each" || call.arguments().is_some() {
            continue;
        }
        let Some(receiver) = call.receiver() else { continue };
        let values = match receiver.as_constant_read_node() {
            Some(read) => {
                let name = constant_id_str(&read.name()).to_string();
                lists.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone())
            }
            None => literal_list(&receiver),
        };
        let Some(values) = values else { continue };
        let Some(block) = call.block().and_then(|b| b.as_block_node()) else { continue };
        let Some(var) = block
            .parameters()
            .and_then(|p| p.as_block_parameters_node())
            .and_then(|p| p.parameters())
            .and_then(|p| p.requireds().iter().next())
            .and_then(|p| p.as_required_parameter_node())
            .map(|p| constant_id_str(&p.name()).to_string())
        else {
            continue;
        };
        let Some(inner) = block.body().and_then(|b| b.as_statements_node()) else { continue };
        let inner: Vec<ruby_prism::Node<'_>> = inner.body().iter().collect();
        let [only] = inner.as_slice() else { continue };
        let Some(require) = only.as_call_node() else { continue };
        let name = constant_id_str(&require.name());
        if require.receiver().is_some() || !matches!(name, "require" | "require_relative") {
            continue;
        }
        let args: Vec<_> = require.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
        let [arg] = args.as_slice() else { continue };
        let features: Option<Vec<String>> = values.iter().map(|value| interpolated(arg, &var, value)).collect();
        let Some(features) = features else { continue };
        for feature in features {
            out.push(ProgramItem::Require { span: span_of(stmt), feature, relative: name == "require_relative" });
        }
    }
}

/// `require "x"` / `require_relative "x"` of a string literal, as
/// `(feature, relative)`.
fn literal_require(call: &ruby_prism::CallNode<'_>) -> Option<(String, bool)> {
    let name = constant_id_str(&call.name());
    if call.receiver().is_some() || call.block().is_some() || !matches!(name, "require" | "require_relative") {
        return None;
    }
    let args: Vec<_> = call.arguments().map(|a| a.arguments().iter().collect()).unwrap_or_default();
    let [one] = args.as_slice() else { return None };
    let feature = one.as_string_node()?;
    Some((String::from_utf8_lossy(feature.unescaped()).into_owned(), name == "require_relative"))
}

/// The values of a literal list of strings or symbols (`%w[a b]`,
/// `[:a, :b]`), frozen or not.
fn literal_list(node: &ruby_prism::Node<'_>) -> Option<Vec<String>> {
    let frozen = node
        .as_call_node()
        .filter(|call| constant_id_str(&call.name()) == "freeze" && call.arguments().is_none())
        .and_then(|call| call.receiver());
    let array = frozen.as_ref().unwrap_or(node).as_array_node()?;
    array
        .elements()
        .iter()
        .map(|e| {
            e.as_string_node()
                .map(|s| String::from_utf8_lossy(s.unescaped()).into_owned())
                .or_else(|| e.as_symbol_node().map(|s| String::from_utf8_lossy(s.unescaped()).into_owned()))
        })
        .collect()
}

/// A string literal with `var` interpolated as `value`
/// (`"adapters/#{n}_adapter"`), or a plain one.
fn interpolated(node: &ruby_prism::Node<'_>, var: &str, value: &str) -> Option<String> {
    if let Some(s) = node.as_string_node() {
        return Some(String::from_utf8_lossy(s.unescaped()).into_owned());
    }
    let interpolated = node.as_interpolated_string_node()?;
    let mut out = String::new();
    for part in interpolated.parts().iter() {
        if let Some(s) = part.as_string_node() {
            out.push_str(&String::from_utf8_lossy(s.unescaped()));
            continue;
        }
        let embedded = part.as_embedded_statements_node()?;
        let statements: Vec<ruby_prism::Node<'_>> = embedded.statements()?.body().iter().collect();
        let [only] = statements.as_slice() else { return None };
        let read = only.as_local_variable_read_node()?;
        if constant_id_str(&read.name()) != var {
            return None;
        }
        out.push_str(value);
    }
    Some(out)
}

/// A top-level statement holding a `return` outside a `def` or lambda,
/// which ends the file's load.
fn is_file_return(stmt: &ruby_prism::Node<'_>) -> bool {
    use ruby_prism::Visit;
    #[derive(Default)]
    struct Finds(bool);
    impl<'pr> Visit<'pr> for Finds {
        fn visit_return_node(&mut self, _: &ruby_prism::ReturnNode<'pr>) {
            self.0 = true;
        }
        fn visit_def_node(&mut self, _: &ruby_prism::DefNode<'pr>) {}
        fn visit_lambda_node(&mut self, _: &ruby_prism::LambdaNode<'pr>) {}
        fn visit_class_node(&mut self, _: &ruby_prism::ClassNode<'pr>) {}
        fn visit_module_node(&mut self, _: &ruby_prism::ModuleNode<'pr>) {}
    }
    let mut finds = Finds::default();
    finds.visit(stmt);
    finds.0
}

/// Whether a statement other than a declaration holds a `class`,
/// `module` or `def` (one inside an `if`, a block, a `begin`): the
/// expression ingest would read it as nil, and the library-class walk
/// does not reach it.
fn declares_inside(stmt: &ruby_prism::Node<'_>) -> bool {
    use ruby_prism::Visit;
    #[derive(Default)]
    struct Finds(bool);
    impl<'pr> Visit<'pr> for Finds {
        fn visit_class_node(&mut self, _: &ruby_prism::ClassNode<'pr>) {
            self.0 = true;
        }
        fn visit_module_node(&mut self, _: &ruby_prism::ModuleNode<'pr>) {
            self.0 = true;
        }
        fn visit_def_node(&mut self, _: &ruby_prism::DefNode<'pr>) {
            self.0 = true;
        }
    }
    let mut finds = Finds::default();
    finds.visit(stmt);
    finds.0
}
