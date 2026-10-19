//! A lambda keeps its whole signature through ingest and Ruby emit:
//! optional positionals, keywords (required and optional) and a keyword
//! rest, named or anonymous. Dropping one is not a degradation: the body
//! still reads the name, so the emitted program raises `NameError`.
//! Behavior on CRuby and Spinel is pinned in `emit_and_run` and
//! `spinel_toolchain`.

use roundhouse::emit::ruby as ruby_emit;
use roundhouse::expr::Expr;
use roundhouse::ingest;

fn ingest_snippet(source: &str) -> Expr {
    let result = ingest::prism::parse(source.as_bytes(), "<input>");
    let root = result.node();
    let program = root.as_program_node().expect("a Ruby program");
    ingest::ingest_expr(&program.statements().as_node(), "<input>").expect("ingest")
}

/// Ruby emit renders the signature as written, and re-ingesting the
/// emitted text gives the same IR.
fn assert_signature_kept(source: &str) {
    let first = ingest_snippet(source);
    let emitted = ruby_emit::emit_expr(&first);
    assert_eq!(emitted.trim(), source, "signature changed");
    assert_eq!(ingest_snippet(&emitted), first, "IR changed across emit and re-ingest:\n{emitted}");
}

#[test]
fn an_optional_positional_is_kept() {
    assert_signature_kept(r##"f = ->(name, size = 18) { "#{name}:#{size}" }"##);
}

#[test]
fn required_and_optional_keywords_are_kept() {
    assert_signature_kept("f = ->(ctx, key:, limit: 10) { ctx[key].first(limit) }");
}

#[test]
fn a_named_keyword_rest_is_kept() {
    assert_signature_kept("f = ->(ctx, **opts) { opts }");
}

#[test]
fn an_anonymous_keyword_rest_is_kept() {
    assert_signature_kept("f = ->(represented:, **) { represented }");
}

#[test]
fn every_kind_together_is_kept_in_ruby_order() {
    assert_signature_kept("f = ->(a, b = 2, *rest, key: 1, **opts) { [a, b, rest, key, opts] }");
}

/// A target whose emitter reads only the required names reports the
/// lambda as unsupported instead of emitting it without the rest.
/// Crystal stands in for the strict targets.
#[test]
fn a_strict_target_refuses_instead_of_dropping() {
    let app = include_str!("support/lambda_signatures.rs");
    assert!(app.contains("label = ->(name, size = 18)"), "contract moved");
    let tree = std::collections::HashMap::from([
        (std::path::PathBuf::from("db/schema.rb"), b"ActiveRecord::Schema.define(version: 1) do\n  create_table :articles do |t|\n    t.string :title\n  end\nend\n".to_vec()),
        (std::path::PathBuf::from("config/routes.rb"), b"Rails.application.routes.draw do\nend\n".to_vec()),
        (std::path::PathBuf::from("app/models/article.rb"), b"class Article < ApplicationRecord\n  def self.label(name)\n    f = ->(n, size = 18) { \"#{n}:#{size}\" }\n    f.call(name)\n  end\nend\n".to_vec()),
    ]);
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    roundhouse::session::analyze_and_lower(&mut app);
    for (target, refused) in [
        (roundhouse::project::BuildTarget::Crystal, true),
        (roundhouse::project::BuildTarget::Spinel, false),
        (roundhouse::project::BuildTarget::Ruby, false),
    ] {
        let (_, diags) = roundhouse::emit::diagnostics::scope(|| {
            roundhouse::project::target_files(&app, std::path::Path::new("."), target)
        });
        let refusal = diags.iter().any(|d| d.message.contains("lambda optional, keyword or keyword-rest parameters"));
        assert_eq!(refusal, refused, "{target:?}: {diags:?}");
    }
}

/// A block with an optional parameter still binds its required ones from
/// the receiver: `[1, 2].map { |n, bonus = 2| n + bonus }` types `n` as the
/// element type; left unbound it was an unresolved type.
#[test]
fn a_block_with_an_optional_parameter_keeps_its_element_type() {
    let tree = std::collections::HashMap::from([
        (std::path::PathBuf::from("db/schema.rb"), b"ActiveRecord::Schema.define(version: 1) do\n  create_table :articles do |t|\n    t.string :title\n  end\nend\n".to_vec()),
        (std::path::PathBuf::from("config/routes.rb"), b"Rails.application.routes.draw do\nend\n".to_vec()),
        (std::path::PathBuf::from("app/models/article.rb"), b"class Article < ApplicationRecord\n  def self.sums\n    [1, 2].map { |n, bonus = 2| n + bonus }\n  end\nend\n".to_vec()),
    ]);
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let mut diags = roundhouse::session::analyze_and_lower(&mut app);
    diags.extend(roundhouse::analyze::diagnose(&app));
    let unresolved: Vec<_> = diags
        .iter()
        .filter(|d| matches!(d.kind, roundhouse::diagnostic::DiagnosticKind::UnresolvedType { .. }))
        .collect();
    assert!(unresolved.is_empty(), "{unresolved:#?}");
}

/// Issue #719: a lambda stored in a local takes parameter types from the
/// `.call` sites in the same body. The result of `.call` is that body
/// type, and strict targets invoke the function instead of a `call` method.
#[test]
fn a_local_lambda_call_types_its_parameters_from_the_call() {
    let source = "class Article < ApplicationRecord\n  def self.plain_lambda\n    double = ->(n) { n * 2 }\n    double.call(4)\n  end\nend\n";
    let tree = std::collections::HashMap::from([
        (std::path::PathBuf::from("db/schema.rb"), b"ActiveRecord::Schema.define(version: 1) do\n  create_table :articles do |t|\n    t.string :title\n  end\nend\n".to_vec()),
        (std::path::PathBuf::from("config/routes.rb"), b"Rails.application.routes.draw do\nend\n".to_vec()),
        (std::path::PathBuf::from("app/models/article.rb"), source.as_bytes().to_vec()),
    ]);
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let mut diags = roundhouse::session::analyze_and_lower(&mut app);
    diags.extend(roundhouse::analyze::diagnose(&app));
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(d.severity, roundhouse::diagnostic::Severity::Error)
                || matches!(d.kind, roundhouse::diagnostic::DiagnosticKind::UnresolvedType { .. })
        })
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    let body = app.models.iter().flat_map(|m| m.methods()).find(|m| m.name.as_str() == "plain_lambda").expect("method").body.clone();
    match lambda_fn_ty(&body) {
        Some(roundhouse::ty::Ty::Fn { params, ret, .. }) => {
            assert_eq!(params.len(), 1, "{params:?}");
            assert_eq!(*params[0].ty, roundhouse::ty::Ty::Int);
            assert_eq!(*ret, roundhouse::ty::Ty::Int);
        }
        other => panic!("lambda type: {other:?}"),
    }
    assert_eq!(call_ty(&body), Some(roundhouse::ty::Ty::Int), "call result");

    for target in [
        roundhouse::project::BuildTarget::Typescript,
        roundhouse::project::BuildTarget::Rust,
        roundhouse::project::BuildTarget::Kotlin,
        roundhouse::project::BuildTarget::Go,
    ] {
        let (files, diags) = roundhouse::emit::diagnostics::scope(|| {
            roundhouse::project::target_files(&app, std::path::Path::new("."), target)
        });
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == roundhouse::diagnostic::Severity::Error).collect();
        assert!(errors.is_empty(), "{target:?}: {errors:#?}");
        let files = files.expect("files");
        let src = files
            .iter()
            .filter(|(path, _)| {
                let path = path.to_lowercase();
                path.contains("article") && !path.ends_with(".map") && !path.ends_with(".rbs")
            })
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            src.contains("double(4") || src.contains("func(n int64)"),
            "{target:?} did not invoke the lambda:\n{src}"
        );
        assert!(!src.contains("double.call") && !src.contains("double.Call"), "{target:?}:\n{src}");
    }
}

#[test]
fn a_lambda_call_inside_another_lambda_seeds_both() {
    let source = "class Article < ApplicationRecord\n  def self.plain_lambda\n    add = ->(n) { n * 2 }\n    run = ->(x) { add.call(x) }\n    run.call(4)\n  end\nend\n";
    let tree = std::collections::HashMap::from([
        (std::path::PathBuf::from("db/schema.rb"), b"ActiveRecord::Schema.define(version: 1) do\n  create_table :articles do |t|\n    t.string :title\n  end\nend\n".to_vec()),
        (std::path::PathBuf::from("config/routes.rb"), b"Rails.application.routes.draw do\nend\n".to_vec()),
        (std::path::PathBuf::from("app/models/article.rb"), source.as_bytes().to_vec()),
    ]);
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let mut diags = roundhouse::session::analyze_and_lower(&mut app);
    diags.extend(roundhouse::analyze::diagnose(&app));
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(d.severity, roundhouse::diagnostic::Severity::Error)
                || matches!(d.kind, roundhouse::diagnostic::DiagnosticKind::UnresolvedType { .. })
        })
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    let body = app.models.iter().flat_map(|m| m.methods()).find(|m| m.name.as_str() == "plain_lambda").expect("method").body.clone();
    let tys = lambda_fn_tys(&body);
    assert_eq!(tys.len(), 2, "{tys:?}");
    for ty in tys {
        let roundhouse::ty::Ty::Fn { params, ret, .. } = ty else { panic!("{ty:?}") };
        assert_eq!(params.len(), 1);
        assert_eq!(*params[0].ty, roundhouse::ty::Ty::Int);
        assert_eq!(*ret, roundhouse::ty::Ty::Int);
    }
    assert!(call_tys(&body).iter().all(|ty| ty == &roundhouse::ty::Ty::Int), "call results");
}

#[test]
fn a_nil_seed_reassigned_from_a_lambda_call_includes_the_result() {
    let source = "class Article < ApplicationRecord\n  def self.plain_lambda\n    x = nil\n    double = ->(n) { n * 2 }\n    x = double.call(4)\n    x\n  end\nend\n";
    let tree = std::collections::HashMap::from([
        (std::path::PathBuf::from("db/schema.rb"), b"ActiveRecord::Schema.define(version: 1) do\n  create_table :articles do |t|\n    t.string :title\n  end\nend\n".to_vec()),
        (std::path::PathBuf::from("config/routes.rb"), b"Rails.application.routes.draw do\nend\n".to_vec()),
        (std::path::PathBuf::from("app/models/article.rb"), source.as_bytes().to_vec()),
    ]);
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).expect("ingest");
    let mut diags = roundhouse::session::analyze_and_lower(&mut app);
    diags.extend(roundhouse::analyze::diagnose(&app));
    let bad: Vec<_> = diags
        .iter()
        .filter(|d| {
            matches!(d.severity, roundhouse::diagnostic::Severity::Error)
                || matches!(d.kind, roundhouse::diagnostic::DiagnosticKind::UnresolvedType { .. })
        })
        .collect();
    assert!(bad.is_empty(), "{bad:#?}");
    let body = app.models.iter().flat_map(|m| m.methods()).find(|m| m.name.as_str() == "plain_lambda").expect("method").body.clone();
    let ty = result_ty(&body).unwrap_or_else(|| panic!("no result type on {}", body.node.kind_str()));
    let roundhouse::ty::Ty::Union { variants } = &ty else { panic!("expected Nil | Integer, got {ty:?}") };
    assert!(variants.contains(&roundhouse::ty::Ty::Int), "{ty:?}");
    assert!(variants.contains(&roundhouse::ty::Ty::Nil), "{ty:?}");
    assert!(!variants.iter().any(|v| matches!(v, roundhouse::ty::Ty::Untyped)), "{ty:?}");
}

fn result_ty(expr: &roundhouse::expr::Expr) -> Option<roundhouse::ty::Ty> {
    if let roundhouse::expr::ExprNode::Seq { exprs } = &*expr.node {
        return exprs.last().and_then(result_ty);
    }
    expr.ty.clone()
}

fn lambda_fn_tys(expr: &roundhouse::expr::Expr) -> Vec<roundhouse::ty::Ty> {
    let mut found = Vec::new();
    if matches!(&*expr.node, roundhouse::expr::ExprNode::Lambda { .. }) {
        if let Some(ty) = expr.ty.clone() {
            found.push(ty);
        }
    }
    expr.node.for_each_child(&mut |child| found.extend(lambda_fn_tys(child)));
    found
}

fn lambda_fn_ty(expr: &roundhouse::expr::Expr) -> Option<roundhouse::ty::Ty> {
    if matches!(&*expr.node, roundhouse::expr::ExprNode::Lambda { .. }) {
        return expr.ty.clone();
    }
    let mut found = None;
    expr.node.for_each_child(&mut |child| {
        if found.is_none() {
            found = lambda_fn_ty(child);
        }
    });
    found
}

fn call_tys(expr: &roundhouse::expr::Expr) -> Vec<roundhouse::ty::Ty> {
    let mut found = Vec::new();
    if let roundhouse::expr::ExprNode::Send { method, .. } = &*expr.node {
        if matches!(method.as_str(), "call" | "[]") {
            if let Some(ty) = expr.ty.clone() {
                found.push(ty);
            }
        }
    }
    expr.node.for_each_child(&mut |child| found.extend(call_tys(child)));
    found
}

fn call_ty(expr: &roundhouse::expr::Expr) -> Option<roundhouse::ty::Ty> {
    if let roundhouse::expr::ExprNode::Send { method, .. } = &*expr.node {
        if method.as_str() == "call" {
            return expr.ty.clone();
        }
    }
    let mut found = None;
    expr.node.for_each_child(&mut |child| {
        if found.is_none() {
            found = call_ty(child);
        }
    });
    found
}

/// `**nil` (no keywords accepted) has no representation; dropping it would
/// let the emitted lambda accept keywords the source rejects, so ingest
/// refuses it.
#[test]
fn a_no_keywords_parameter_is_refused() {
    let result = ingest::prism::parse(b"f = ->(a, **nil) { a }", "<input>");
    let program = result.node().as_program_node().expect("a Ruby program");
    assert!(ingest::ingest_expr(&program.statements().as_node(), "<input>").is_err());
}
