//! `accepts_nested_attributes_for :assoc, …` on a model (#1335, part 1:
//! the writer and building new records).
//!
//! Rails (activerecord 8.1.4) defines `<assoc>_attributes=` for each
//! association named. For a has_many it takes an Array of hashes or a hash
//! of hashes (`{"0" => {…}}`) and builds one record per entry; for a
//! has_one or a belongs_to, one hash and one record. The records are
//! validated with the parent (an invalid one fails the parent's `save`,
//! its errors under the association: "Comments body can't be blank") and
//! saved with it: a has_many/has_one record after the parent, carrying its
//! foreign key; a belongs_to record before it, the parent then carrying
//! its id.
//!
//! On the ruby family the declaration becomes ordinary model source,
//! ingested like the app's own: the writer, a `validate` method and a save callback per
//! association. An entry with an `id` (updating an existing record) and
//! `_destroy` are part 2; an entry with an `id` raises where Rails would
//! update. Options other than `allow_destroy:` (`reject_if:`, `limit:`,
//! `update_only:`) keep the whole declaration a reported gap, as do a
//! name that is not a plain has_many/has_one/belongs_to of this model.
//! Other targets keep the declaration as the reported gap it was.

use crate::app::App;
use crate::dialect::{Association, Model, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

/// The associations one declaration names, when every one of them and
/// every option is reproduced.
fn declaration(expr: &Expr) -> Option<Vec<Symbol>> {
    let ExprNode::Send { recv: None, method, args, block: None, .. } = &*expr.node else { return None };
    if method.as_str() != "accepts_nested_attributes_for" {
        return None;
    }
    let mut names = Vec::new();
    for arg in args {
        match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } => names.push(value.clone()),
            ExprNode::Hash { entries, .. } => {
                for (key, value) in entries {
                    let ExprNode::Lit { value: Literal::Sym { value: key } } = &*key.node else { return None };
                    match (key.as_str(), &*value.node) {
                        // `_destroy` is part 2; an unmarked entry builds
                        // the same with or without it.
                        ("allow_destroy", ExprNode::Lit { value: Literal::Bool { .. } }) => {}
                        _ => return None,
                    }
                }
            }
            _ => return None,
        }
    }
    (!names.is_empty()).then_some(names)
}

fn is_declaration(item: &ModelBodyItem) -> bool {
    matches!(item, ModelBodyItem::Unknown { expr, .. }
        if matches!(&*expr.node, ExprNode::Send { recv: None, method, .. } if method.as_str() == "accepts_nested_attributes_for"))
}

/// One association a declaration names, as the generated source needs it.
struct Target {
    name: String,
    class: String,
    foreign_key: String,
    kind: Kind,
    /// The target's belongs_to back to the parent (by foreign key), whose
    /// "must exist" Rails never reports for a record it builds here.
    inverse: Option<String>,
}

#[derive(PartialEq)]
enum Kind {
    HasMany,
    HasOne,
    BelongsTo,
}

fn targets(model: &Model, names: &[Symbol], models: &[Model]) -> Option<Vec<Target>> {
    names
        .iter()
        .map(|name| {
            let assoc = model.associations().find(|a| a.name() == name)?;
            let (kind, class, foreign_key) = match assoc {
                Association::HasMany { target, foreign_key, through: None, .. } => (Kind::HasMany, target, foreign_key),
                Association::HasOne { target, foreign_key, through: None, .. } => (Kind::HasOne, target, foreign_key),
                Association::BelongsTo { target, foreign_key, polymorphic: false, .. } => (Kind::BelongsTo, target, foreign_key),
                _ => return None,
            };
            // The target is one of the app's models (its constructor and
            // `save!` are the ones the generated source calls).
            let target_model = models.iter().find(|m| &m.name == class)?;
            let inverse = (kind != Kind::BelongsTo)
                .then(|| {
                    target_model.associations().find_map(|a| match a {
                        Association::BelongsTo { name, foreign_key: fk, polymorphic: false, .. } if fk == foreign_key => {
                            Some(name.as_str().to_string())
                        }
                        _ => None,
                    })
                })
                .flatten();
            Some(Target {
                name: name.as_str().to_string(),
                class: class.0.as_str().to_string(),
                foreign_key: foreign_key.as_str().to_string(),
                kind,
                inverse,
            })
        })
        .collect()
}

/// `"comments"` → `"Comments"`, as `humanize` reads an association name.
fn humanize(name: &str) -> String {
    let spaced = name.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The model source for one association.
fn source(target: &Target) -> String {
    let Target { name, class, foreign_key, kind, inverse } = target;
    let ivar = format!("@__nested_{name}");
    let human = humanize(name);
    let id_guard = format!(
        "      raise NotImplementedError, \"{name}_attributes: an entry with an id updates an existing record, which is not supported yet (#1335)\" if attrs.key?(:id) && !attrs[:id].to_s.empty?\n"
    );
    let writer = if *kind == Kind::HasMany {
        format!(
            "  def {name}_attributes=(attributes)\n    {ivar} = [] if {ivar}.nil?\n    entries = attributes.is_a?(Hash) ? attributes.values : attributes\n    entries.each do |entry|\n      attrs = {{}}\n      entry.each {{ |key, value| attrs[key.to_sym] = value }}\n{id_guard}      attrs.delete(:_destroy)\n      {ivar} << {class}.new(attrs)\n    end\n  end\n\n"
        )
    } else {
        format!(
            "  def {name}_attributes=(attributes)\n    attrs = {{}}\n    attributes.each {{ |key, value| attrs[key.to_sym] = value }}\n{id_guard}    attrs.delete(:_destroy)\n    {ivar} = [{class}.new(attrs)]\n  end\n\n"
        )
    };
    let skip_inverse = inverse
        .as_ref()
        .map(|inverse| format!("        next if attribute == \"{inverse}\"\n"))
        .unwrap_or_default();
    let validate = format!(
        "  def __nested_{name}_valid\n    return if {ivar}.nil?\n    {ivar}.each do |entry|\n      next unless entry.is_a?({class})\n      child = entry\n      next if child.valid?\n      child.errors.each do |error|\n        attribute = error.attribute.to_s\n{skip_inverse}        full = \"{human} \" + attribute.tr(\"_\", \" \") + \" \" + error.message\n        errors << full\n        error_details << ActiveRecord::ErrorDetail.new(\"{name}.\" + attribute, error.type.to_s, error.message, full, true, true)\n      end\n    end\n  end\n\n  validate :__nested_{name}_valid\n\n"
    );
    let save = match kind {
        Kind::BelongsTo => format!(
            "  def __nested_{name}_save\n    return if {ivar}.nil?\n    children = {ivar}\n    {ivar} = nil\n    children.each do |child|\n      child.save!\n      self.{foreign_key} = child.id\n    end\n  end\n\n  before_save :__nested_{name}_save\n\n"
        ),
        Kind::HasMany | Kind::HasOne => {
            let reset = if *kind == Kind::HasMany { format!("    reload_{name}\n") } else { String::new() };
            format!(
                "  def __nested_{name}_save\n    return if {ivar}.nil?\n    children = {ivar}\n    {ivar} = nil\n    children.each do |child|\n      child.{foreign_key} = id\n      child.save!\n    end\n{reset}  end\n\n  after_save :__nested_{name}_save\n\n"
            )
        }
    };
    writer + &validate + &save
}

/// Expand every reproducible declaration into model source, in place.
/// Run for a target whose runtime the generated source is verified on
/// (`BuildTarget::lowers_nested_attributes`), after ingest, so a concern's
/// `included do` declaration has been spliced into its includers.
pub(crate) fn expand(app: &mut App) {
    let snapshot: Vec<Model> = app.models.clone();
    for model in &mut app.models {
        let mut generated: Vec<ModelBodyItem> = Vec::new();
        let mut expanded = Vec::new();
        for (index, item) in model.body.iter().enumerate() {
            let ModelBodyItem::Unknown { expr, .. } = item else { continue };
            let Some(names) = declaration(expr) else { continue };
            let Some(targets) = targets(model, &names, &snapshot) else { continue };
            let src = format!(
                "class {}\n{}end\n",
                model.name.0.as_str().replace("::", "__"),
                targets.iter().map(source).collect::<String>()
            );
            let Some(items) = ingest_source(&src, &model.name) else { continue };
            generated.extend(items);
            expanded.push(index);
        }
        if expanded.is_empty() {
            continue;
        }
        let mut index = 0;
        model.body.retain(|item| {
            let keep = !(is_declaration(item) && expanded.contains(&index));
            index += 1;
            keep
        });
        model.body.extend(generated);
    }
}

/// The body items of `src`'s one class, ingested as model source under
/// its own isolated scope (as `delegate` forwarders are).
fn ingest_source(src: &str, owner: &crate::ident::ClassId) -> Option<Vec<ModelBodyItem>> {
    let (items, diags) = crate::ingest::prism::scope(|| -> Option<Vec<ModelBodyItem>> {
        let result = crate::ingest::prism::parse(src.as_bytes(), "<nested_attributes>");
        let root = result.node();
        let class = root.as_program_node()?.statements().body().iter().next()?.as_class_node()?;
        let body = class.body()?;
        let mut out = Vec::new();
        for stmt in super::util::flatten_statements(body) {
            out.extend(
                super::model::ingest_model_body_items(&stmt, owner, "<nested_attributes>", Vec::new(), None, &Default::default())
                    .ok()?,
            );
        }
        Some(out)
    });
    if !diags.is_empty() {
        return None;
    }
    items
}
