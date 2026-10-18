//! String core-ext grounding, one table.
//!
//! `(name, receiver shape, arity, runtime receiver, runtime name)` for
//! `humanize`, `titleize`, `underscore`, `demodulize`, `singularize`,
//! `remove`, `ordinalize`, and `parameterize`. The bodies stay in
//! `runtime/ruby/active_support_inflections.rb` and
//! `runtime/ruby/inflector_ext.rb` (`Inflector.parameterize`); this
//! pass only rewrites the call.
//!
//! `parameterize` stays on model `def` bodies and unknown model-body
//! expressions. The fused view walk does not apply that row, which is
//! what the old pass did.

use crate::app::App;
use crate::expr::{Expr, ExprNode};
use crate::ident::Symbol;
use crate::ty::Ty;

/// Receiver the analyzer must have stamped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReceiverShape {
    /// Exactly `Ty::Str`. `parameterize` does not accept `String | nil`.
    String,
    /// `Ty::Str`, or a union of only `Str` and `Nil` that contains `Str`.
    StringOrNil,
    /// Exactly `Ty::Int`.
    Int,
}

impl ReceiverShape {
    fn matches(self, ty: Option<&Ty>) -> bool {
        match self {
            ReceiverShape::String => matches!(ty, Some(Ty::Str)),
            ReceiverShape::Int => matches!(ty, Some(Ty::Int)),
            ReceiverShape::StringOrNil => match ty {
                Some(Ty::Str) => true,
                Some(Ty::Union { variants }) => {
                    variants.iter().any(|v| matches!(v, Ty::Str))
                        && variants.iter().all(|v| matches!(v, Ty::Str | Ty::Nil))
                }
                _ => false,
            },
        }
    }
}

/// Where a row is grounded. Hooks and views share the inflection rows.
/// `parameterize` is model bodies only, so a view keeps the call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Coverage {
    HooksAndViews,
    ModelBodies,
}

/// Which walk applies a row. The name still lives only in the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Walk {
    /// Fused hook bodies and the controller retype.
    Hooks,
    /// Fused view bodies.
    Views,
    /// Model methods and unknown model-body expressions.
    ModelBodies,
}

/// One string core-ext method.
pub(crate) struct StringCoreExt {
    pub name: &'static str,
    pub receiver: ReceiverShape,
    pub arity: usize,
    pub runtime_receiver: &'static [&'static str],
    pub runtime_name: &'static str,
    pub coverage: Coverage,
    /// Inflection rows stamp `Ty::Str`. `parameterize` left the analyzer's type.
    pub stamp_string: bool,
}

impl StringCoreExt {
    fn applies(&self, walk: Walk) -> bool {
        match (self.coverage, walk) {
            (Coverage::HooksAndViews, Walk::Hooks | Walk::Views) => true,
            (Coverage::ModelBodies, Walk::ModelBodies) => true,
            _ => false,
        }
    }
}

/// The string family. Names are not repeated in the rewrite.
pub(crate) const STRING_CORE_EXT: &[StringCoreExt] = &[
    StringCoreExt {
        name: "humanize",
        receiver: ReceiverShape::StringOrNil,
        arity: 0,
        runtime_receiver: &["ActiveSupport"],
        runtime_name: "humanize",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    StringCoreExt {
        name: "titleize",
        receiver: ReceiverShape::StringOrNil,
        arity: 0,
        runtime_receiver: &["ActiveSupport"],
        runtime_name: "titleize",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    StringCoreExt {
        name: "underscore",
        receiver: ReceiverShape::StringOrNil,
        arity: 0,
        runtime_receiver: &["ActiveSupport"],
        runtime_name: "underscore",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    StringCoreExt {
        name: "demodulize",
        receiver: ReceiverShape::StringOrNil,
        arity: 0,
        runtime_receiver: &["ActiveSupport"],
        runtime_name: "demodulize",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    StringCoreExt {
        name: "singularize",
        receiver: ReceiverShape::StringOrNil,
        arity: 0,
        runtime_receiver: &["ActiveSupport"],
        runtime_name: "singularize",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    // ActiveSupport's `String#remove` takes one or more patterns; the
    // runtime helper takes one. Multi-arg calls stay dynamic.
    StringCoreExt {
        name: "remove",
        receiver: ReceiverShape::StringOrNil,
        arity: 1,
        runtime_receiver: &["ActiveSupport"],
        runtime_name: "remove",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    StringCoreExt {
        name: "ordinalize",
        receiver: ReceiverShape::Int,
        arity: 0,
        runtime_receiver: &["ActiveSupport", "Inflector"],
        runtime_name: "ordinalize",
        coverage: Coverage::HooksAndViews,
        stamp_string: true,
    },
    // Zero-arg `String#parameterize` → `Inflector.parameterize(recv)`.
    // Separator kwargs and untyped receivers stay verbatim.
    StringCoreExt {
        name: "parameterize",
        receiver: ReceiverShape::String,
        arity: 0,
        runtime_receiver: &["Inflector"],
        runtime_name: "parameterize",
        coverage: Coverage::ModelBodies,
        stamp_string: false,
    },
];

/// Model methods and unknown model-body expressions. A block-form
/// callback spliced from a concern is still `Unknown` here (lobsters'
/// `after_initialize` / `TypeID.new(self.class.to_s.parameterize)`).
pub fn apply_parameterize_grounding(app: &mut App) {
    for model in &mut app.models {
        for item in &mut model.body {
            match item {
                crate::dialect::ModelBodyItem::Method { method, .. } => {
                    walk(&mut method.body, Walk::ModelBodies);
                }
                crate::dialect::ModelBodyItem::Unknown { expr, .. } => {
                    walk(expr, Walk::ModelBodies);
                }
                _ => {}
            }
        }
    }
}

/// The same grounding over a body a later pass retyped. The controller
/// lowering runs it after the typed-params rewrite turns `helper[:field]`
/// (untyped) into `helper.field` (String).
pub(crate) fn ground_in_place(expr: &mut Expr) -> bool {
    let mut changed = false;
    walk_flag(expr, Walk::Hooks, &mut changed);
    changed
}

fn walk(expr: &mut Expr, site: Walk) {
    let mut changed = false;
    walk_flag(expr, site, &mut changed);
}

fn walk_flag(expr: &mut Expr, site: Walk, changed: &mut bool) {
    expr.node
        .for_each_child_mut(&mut |child| walk_flag(child, site, changed));
    *changed |= rewrite_node(expr, site);
}

/// Grounds one call on `site`. Fused walks the tree and calls this per node.
pub(crate) fn rewrite_node(expr: &mut Expr, site: Walk) -> bool {
    let Some(matched) = match_rule(expr, site) else {
        return false;
    };
    *expr.node = ExprNode::Send {
        recv: Some(Expr::new(expr.span, ExprNode::Const { path: matched.path })),
        method: matched.method,
        args: matched.args,
        block: None,
        parenthesized: true,
    };
    if matched.stamp_string {
        expr.ty = Some(Ty::Str);
    }
    true
}

struct Matched {
    path: Vec<Symbol>,
    method: Symbol,
    args: Vec<Expr>,
    stamp_string: bool,
}

fn match_rule(expr: &Expr, site: Walk) -> Option<Matched> {
    let ExprNode::Send {
        recv: Some(receiver),
        method,
        args,
        block: None,
        ..
    } = &*expr.node
    else {
        return None;
    };
    let name = method.as_str();
    let rule = STRING_CORE_EXT
        .iter()
        .find(|rule| rule.name == name && rule.applies(site))?;
    if args.len() != rule.arity || !rule.receiver.matches(receiver.ty.as_ref()) {
        return None;
    }
    let mut grounded = Vec::with_capacity(1 + args.len());
    grounded.push(receiver.clone());
    grounded.extend(args.iter().cloned());
    let method = if rule.runtime_name == name {
        method.clone()
    } else {
        Symbol::from(rule.runtime_name)
    };
    Some(Matched {
        path: rule
            .runtime_receiver
            .iter()
            .copied()
            .map(Symbol::from)
            .collect(),
        method,
        args: grounded,
        stamp_string: rule.stamp_string,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "humanize",
        "titleize",
        "underscore",
        "demodulize",
        "singularize",
        "remove",
        "ordinalize",
        "parameterize",
    ];

    #[test]
    fn names_are_the_string_family_once() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in STRING_CORE_EXT {
            assert!(seen.insert(rule.name), "duplicate row {}", rule.name);
            assert_eq!(rule.runtime_name, rule.name);
        }
        assert_eq!(seen, NAMES.iter().copied().collect());
    }

    #[test]
    fn the_rewrite_does_not_hardcode_a_name() {
        let src = include_str!("string_core_ext.rs");
        let (head, after) = src
            .split_once("pub(crate) const STRING_CORE_EXT")
            .expect("table");
        let after_table = after.split_once("];").expect("end of table").1;
        let outside = format!(
            "{head}{}",
            after_table.split("#[cfg(test)]").next().expect("tests")
        );
        let fused = include_str!("fused.rs");
        for name in NAMES {
            let needle = format!("\"{name}\"");
            assert!(
                !outside.contains(&needle),
                "{name} is hardcoded outside the table"
            );
            assert!(!fused.contains(&needle), "fused hardcodes {name}");
        }
        assert!(
            !std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/lower/parameterize.rs"
            ))
            .exists()
        );
        assert!(
            !std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/lower/string_inflections.rs"
            ))
            .exists()
        );
    }

    fn call(receiver_ty: Option<Ty>, method: &str, arity: usize) -> Expr {
        let span = crate::span::Span::synthetic();
        let mut receiver = Expr::new(
            span,
            ExprNode::Lit {
                value: crate::expr::Literal::Str { value: "x".into() },
            },
        );
        receiver.ty = receiver_ty.clone();
        let args = (0..arity)
            .map(|_| {
                Expr::new(
                    span,
                    ExprNode::Lit {
                        value: crate::expr::Literal::Str { value: "-".into() },
                    },
                )
            })
            .collect();
        let mut expr = Expr::new(
            span,
            ExprNode::Send {
                recv: Some(receiver),
                method: Symbol::from(method),
                args,
                block: None,
                parenthesized: false,
            },
        );
        expr.ty = receiver_ty;
        expr
    }

    fn const_path(expr: &Expr) -> Option<Vec<String>> {
        let ExprNode::Send {
            recv: Some(recv), ..
        } = &*expr.node
        else {
            return None;
        };
        let ExprNode::Const { path } = &*recv.node else {
            return None;
        };
        Some(path.iter().map(|s| s.as_str().to_string()).collect())
    }

    #[test]
    fn rows_ground_on_their_walk_only() {
        let mut humanize = call(Some(Ty::Str), "humanize", 0);
        assert!(rewrite_node(&mut humanize, Walk::Views));
        assert_eq!(
            const_path(&humanize).as_deref(),
            Some(&["ActiveSupport".to_string()][..])
        );
        assert_eq!(humanize.ty, Some(Ty::Str));

        let mut nullable = call(
            Some(Ty::Union {
                variants: vec![Ty::Str, Ty::Nil].into(),
            }),
            "humanize",
            0,
        );
        assert!(rewrite_node(&mut nullable, Walk::Hooks));

        let mut ordinal = call(Some(Ty::Int), "ordinalize", 0);
        assert!(rewrite_node(&mut ordinal, Walk::Views));
        assert_eq!(
            const_path(&ordinal).as_deref(),
            Some(&["ActiveSupport".to_string(), "Inflector".to_string()][..])
        );

        let mut remove = call(Some(Ty::Str), "remove", 1);
        assert!(rewrite_node(&mut remove, Walk::Hooks));
        let mut multi = call(Some(Ty::Str), "remove", 2);
        assert!(!rewrite_node(&mut multi, Walk::Hooks));

        let mut parameterize = call(Some(Ty::Str), "parameterize", 0);
        let original_ty = parameterize.ty.clone();
        assert!(
            !rewrite_node(&mut parameterize, Walk::Views),
            "views skip parameterize"
        );
        assert!(!rewrite_node(&mut parameterize, Walk::Hooks));
        assert!(rewrite_node(&mut parameterize, Walk::ModelBodies));
        assert_eq!(
            const_path(&parameterize).as_deref(),
            Some(&["Inflector".to_string()][..])
        );
        assert_eq!(parameterize.ty, original_ty);

        let mut union = call(
            Some(Ty::Union {
                variants: vec![Ty::Str, Ty::Nil].into(),
            }),
            "parameterize",
            0,
        );
        assert!(!rewrite_node(&mut union, Walk::ModelBodies));
    }

    #[test]
    fn views_keep_parameterize_and_still_ground_humanize() {
        use std::collections::HashMap;
        use std::path::PathBuf;

        let files: HashMap<PathBuf, Vec<u8>> = [
            (
                "db/schema.rb",
                "ActiveRecord::Schema.define do\n  create_table \"stories\", force: :cascade do |t|\n    t.string \"title\"\n  end\nend\n",
            ),
            ("config/routes.rb", "Rails.application.routes.draw do\nend\n"),
            (
                "app/models/story.rb",
                "class Story < ApplicationRecord\n  def title_as_url\n    \"Hello World\".parameterize\n  end\n  scope :slugged, -> { where(title: \"Hello World\".parameterize) }\nend\n",
            ),
            (
                "app/views/stories/show.html.erb",
                "<p><%= \"Hello World\".parameterize %></p>\n<p><%= \"Hello World\".humanize %></p>\n",
            ),
        ]
        .into_iter()
        .map(|(path, source)| (PathBuf::from(path), source.as_bytes().to_vec()))
        .collect();
        let mut app = crate::ingest::ingest_app_from_tree(files).expect("ingest");
        crate::session::analyze_and_lower(&mut app);
        let emitted = crate::emit::ruby::emit_spinel(&app);
        let file = |suffix: &str| {
            emitted
                .iter()
                .find(|f| f.path.ends_with(suffix))
                .unwrap_or_else(|| {
                    panic!(
                        "no {suffix} in {:?}",
                        emitted.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
                    )
                })
                .content
                .as_str()
        };
        let model = file("app/models/story.rb");
        assert!(
            model.contains("Inflector.parameterize(\"Hello World\")"),
            "model method grounds parameterize:\n{model}"
        );
        assert!(
            model.contains("\"Hello World\".parameterize"),
            "a scope is outside the model-body walk:\n{model}"
        );
        let view = file("app/views/stories/show.rb");
        assert!(
            view.contains("\"Hello World\".parameterize"),
            "views keep String#parameterize:\n{view}"
        );
        assert!(
            !view.contains("Inflector.parameterize"),
            "views do not gain Inflector.parameterize:\n{view}"
        );
        assert!(
            view.contains("ActiveSupport.humanize(\"Hello World\")"),
            "views still ground humanize:\n{view}"
        );
    }
}
