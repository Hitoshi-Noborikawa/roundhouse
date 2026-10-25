//! `accepts_nested_attributes_for :assoc, …` on a model (#1335, parts 1
//! and 2: building new records, and updating or destroying existing ones).
//!
//! Rails (activerecord 8.1.4) defines `<assoc>_attributes=` for each
//! association named. For a has_many it takes an Array of hashes or a hash
//! of hashes (`{"0" => {…}}`); for a has_one or a belongs_to, one hash. An
//! entry without an `id` builds a record; one with an `id` updates that
//! record of the association (`RecordNotFound` when it is not one), and,
//! under `allow_destroy: true`, a truthy `_destroy` destroys it. The
//! records are validated with the parent (an invalid one fails the
//! parent's `save`, its errors under the association: "Comments body can't
//! be blank") and saved with it: a has_many/has_one record after the
//! parent, carrying its foreign key; a belongs_to record before it, the
//! parent then carrying its id.
//!
//! On the ruby family the declaration becomes ordinary model source,
//! ingested like the app's own: the writer, a `validate` method and save
//! callbacks per association. An updated record's columns are assigned
//! through `[]=`; a non-column key is not routed to a writer. Options other
//! than `allow_destroy:` (`reject_if:`, `limit:`, `update_only:`) keep the
//! whole declaration a reported gap, as do a name that is not a plain
//! has_many/has_one/belongs_to of this model. Other targets keep the
//! declaration as the reported gap it was.

use crate::app::App;
use crate::dialect::{Association, Dependent, Model, ModelBodyItem};
use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::Symbol;

/// The associations one declaration names, when every one of them and
/// every option is reproduced.
fn declaration(expr: &Expr) -> Option<(Vec<Symbol>, bool)> {
    let ExprNode::Send { recv: None, method, args, block: None, .. } = &*expr.node else { return None };
    if method.as_str() != "accepts_nested_attributes_for" {
        return None;
    }
    let mut names = Vec::new();
    let mut allow_destroy = false;
    for arg in args {
        match &*arg.node {
            ExprNode::Lit { value: Literal::Sym { value } } => names.push(value.clone()),
            ExprNode::Hash { entries, .. } => {
                for (key, value) in entries {
                    let ExprNode::Lit { value: Literal::Sym { value: key } } = &*key.node else { return None };
                    match (key.as_str(), &*value.node) {
                        ("allow_destroy", ExprNode::Lit { value: Literal::Bool { value } }) => allow_destroy = *value,
                        _ => return None,
                    }
                }
            }
            _ => return None,
        }
    }
    (!names.is_empty()).then_some((names, allow_destroy))
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
    /// `allow_destroy: true`: a truthy `_destroy` marks the record.
    allow_destroy: bool,
    /// The declaring model, for `RecordNotFound`'s message.
    owner: String,
    /// `as:`: the target's `<as>_type` column and the value Rails writes
    /// there, the owner's `polymorphic_name`.
    polymorphic_type: Option<(String, String)>,
    /// has_one: what an entry without an `id` does to the record already
    /// there, by the association's `dependent:`.
    replace: Replace,
}

/// Rails' has_one replacement (`HasOneAssociation#remove_target!`).
#[derive(PartialEq, Clone, Copy)]
enum Replace {
    /// has_many / belongs_to: nothing is replaced.
    No,
    Destroy,
    Delete,
    /// No `dependent:`, `:nullify` or a `:restrict_*`: the old record's
    /// key (and `as:` type) is set to nil and saved.
    Nullify,
}

#[derive(PartialEq)]
enum Kind {
    HasMany,
    HasOne,
    BelongsTo,
}

fn targets(model: &Model, names: &[Symbol], allow_destroy: bool, models: &[Model]) -> Option<Vec<Target>> {
    names
        .iter()
        .map(|name| {
            let assoc = model.associations().find(|a| a.name() == name)?;
            // A custom `primary_key:` on either side keys the record by
            // another column than the `id` the generated source reads.
            let (kind, class, foreign_key, as_interface, replace) = match assoc {
                Association::HasMany { target, foreign_key, through: None, primary_key: None, as_interface, .. } => {
                    (Kind::HasMany, target, foreign_key, as_interface, Replace::No)
                }
                Association::HasOne { target, foreign_key, through: None, primary_key: None, as_interface, dependent, .. } => {
                    let replace = match dependent {
                        Dependent::Destroy => Replace::Destroy,
                        Dependent::Delete => Replace::Delete,
                        Dependent::None | Dependent::Nullify | Dependent::Restrict => Replace::Nullify,
                        Dependent::DestroyAsync | Dependent::DeleteAll => return None,
                    };
                    (Kind::HasOne, target, foreign_key, as_interface, replace)
                }
                Association::BelongsTo { target, foreign_key, polymorphic: false, primary_key: None, .. } => {
                    (Kind::BelongsTo, target, foreign_key, &None, Replace::No)
                }
                _ => return None,
            };
            // The target is one of the app's models (its constructor and
            // `save!` are the ones the generated source calls).
            let target_model = models.iter().find(|m| &m.name == class)?;
            // A model keyed by another column (`self.primary_key = …`) too.
            let custom_key = |m: &Model| m.primary_key.as_ref().is_some_and(|k| k.as_str() != "id");
            if custom_key(model) || custom_key(target_model) {
                return None;
            }
            // `as: :addressable`: the target's `belongs_to :addressable,
            // polymorphic: true` is the inverse, and its `_type` column holds
            // the owner's `polymorphic_name` (its STI base class).
            let inverse = (kind != Kind::BelongsTo)
                .then(|| {
                    target_model.associations().find_map(|a| match (a, as_interface) {
                        (Association::BelongsTo { name, polymorphic: true, .. }, Some(as_name)) if name == as_name => {
                            Some(name.as_str().to_string())
                        }
                        (Association::BelongsTo { name, foreign_key: fk, polymorphic: false, .. }, None) if fk == foreign_key => {
                            Some(name.as_str().to_string())
                        }
                        _ => None,
                    })
                })
                .flatten();
            let polymorphic_type = as_interface.as_ref().map(|as_name| {
                // An STI subclass is a library class inheriting this
                // writer, so the declaring model is the base Rails names.
                (format!("{}_type", as_name.as_str()), model.name.0.as_str().to_string())
            });
            Some(Target {
                name: name.as_str().to_string(),
                class: class.0.as_str().to_string(),
                foreign_key: foreign_key.as_str().to_string(),
                kind,
                inverse,
                allow_destroy,
                owner: model.name.0.as_str().to_string(),
                polymorphic_type,
                replace,
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
///
/// An entry without an `id` builds a record; one with an `id` updates
/// that record of the association (`RecordNotFound` when it is not one),
/// its columns assigned now and saved with the parent; with
/// `allow_destroy: true` a truthy `_destroy` (ActiveModel's boolean cast)
/// destroys it instead, and an entry without an `id` so marked builds
/// nothing. A has_one record built replaces the one already there, by
/// `dependent:`, before the parent saves; a required belongs_to marked
/// for destruction fails the parent's "must exist" (`__nested_<name>_marked`,
/// read by the required check). Held records are validated with the parent, then saved:
/// has_many/has_one ones after it, with their foreign key, belongs_to
/// ones before it, the parent carrying the id.
fn source(target: &Target) -> String {
    let Target { name, class, foreign_key, kind, inverse, allow_destroy, owner, polymorphic_type, replace } = target;
    let ivar = format!("@__nested_{name}");
    let doomed = format!("@__nested_{name}_destroy");
    let replaced = format!("@__nested_{name}_replaced");
    let human = humanize(name);
    let destroy = if *allow_destroy {
        "!attrs[:_destroy].nil? && ![\"\", \"0\", \"f\", \"F\", \"false\", \"FALSE\", \"off\", \"OFF\"].include?(attrs[:_destroy].to_s)"
    } else {
        "false"
    };
    let not_found = format!(
        "raise ActiveRecord::RecordNotFound, \"Couldn't find {class} with ID=#{{id}} for {owner} with ID=#{{self.id}}\""
    );
    let decode = format!(
        "attrs = {{}}\n      entry.each {{ |key, value| attrs[key.to_sym] = value }}\n      destroy = {destroy}\n      attrs.delete(:_destroy)\n      id = attrs.delete(:id)\n"
    );
    let writer = if *kind == Kind::HasMany {
        format!(
            "  def {name}_attributes=(attributes)\n    {ivar} = [] if {ivar}.nil?\n    {doomed} = [] if {doomed}.nil?\n    entries = attributes.is_a?(Hash) ? attributes.values : attributes\n    entries.each do |entry|\n      {decode}      if id.nil? || id.to_s.empty?\n        {ivar} << {class}.new(attrs) unless destroy\n      else\n        existing = nil\n        {name}.each {{ |record| existing = record if record.id.to_s == id.to_s }}\n        {not_found} if existing.nil?\n        if destroy\n          {doomed} << existing\n        else\n          attrs.each {{ |key, value| existing[key] = value }}\n          {ivar} << existing\n        end\n      end\n    end\n  end\n\n"
        )
    } else {
        // has_one: the record already there is replaced by the one built.
        let build = if *replace == Replace::No {
            format!("      {ivar} << {class}.new(attrs) unless destroy\n")
        } else {
            format!("      unless destroy\n        {ivar} << {class}.new(attrs)\n        {replaced} = {name}\n      end\n")
        };
        format!(
            "  def {name}_attributes=(attributes)\n    entry = attributes\n    {ivar} = []\n    {doomed} = []\n    {decode}    if id.nil? || id.to_s.empty?\n{build}    else\n      existing = {name}\n      {not_found} if existing.nil? || existing.id.to_s != id.to_s\n      if destroy\n        {doomed} << existing\n      else\n        attrs.each {{ |key, value| existing[key] = value }}\n        {ivar} << existing\n      end\n    end\n  end\n\n"
        )
    };
    let skip_inverse = inverse
        .as_ref()
        .map(|inverse| format!("        next if attribute == \"{inverse}\"\n"))
        .unwrap_or_default();
    let validate = format!(
        "  def __nested_{name}_valid\n    return if {ivar}.nil?\n    {ivar}.each do |entry|\n      next unless entry.is_a?({class})\n      child = entry\n      next if child.valid?\n      child.errors.each do |error|\n        attribute = error.attribute.to_s\n{skip_inverse}        full = \"{human} \" + attribute.tr(\"_\", \" \") + \" \" + error.message\n        errors << full\n        error_details << ActiveRecord::ErrorDetail.new(\"{name}.\" + attribute, error.type.to_s, error.message, full, true, true)\n      end\n    end\n  end\n\n  validate :__nested_{name}_valid\n\n"
    );
    let destroy_doomed = format!(
        "    unless {doomed}.nil?\n      gone = {doomed}\n      {doomed} = nil\n      gone.each {{ |record| record.destroy }}\n    end\n"
    );
    let save = match kind {
        Kind::BelongsTo => format!(
            "  def __nested_{name}_save\n    unless {ivar}.nil?\n      children = {ivar}\n      {ivar} = nil\n      children.each do |child|\n        child.save!\n        self.{foreign_key} = child.id\n      end\n    end\n    self.{foreign_key} = nil unless {doomed}.nil? || {doomed}.empty?\n  end\n\n  before_save :__nested_{name}_save\n\n  def __nested_{name}_destroy\n{destroy_doomed}  end\n\n  after_save :__nested_{name}_destroy\n\n  def __nested_{name}_marked\n    !{doomed}.nil? && !{doomed}.empty?\n  end\n\n"
        ),
        Kind::HasMany | Kind::HasOne => {
            let reset = if *kind == Kind::HasMany { format!("    reload_{name}\n") } else { String::new() };
            let set_type = polymorphic_type
                .as_ref()
                .map(|(column, owner_type)| format!("        child.{column} = \"{owner_type}\"\n"))
                .unwrap_or_default();
            let clear_type = polymorphic_type
                .as_ref()
                .map(|(column, _)| format!("      old.{column} = nil\n"))
                .unwrap_or_default();
            let remove = match replace {
                Replace::No => String::new(),
                Replace::Destroy => "      old.destroy\n".to_string(),
                Replace::Delete => "      old.delete\n".to_string(),
                Replace::Nullify => format!(
                    "      old.{foreign_key} = nil\n{clear_type}      raise ActiveRecord::RecordNotSaved, \"Failed to remove the existing associated {name}. The record failed to save after its foreign key was set to nil.\" unless old.save\n"
                ),
            };
            let replace_old = if remove.is_empty() {
                String::new()
            } else {
                // Before the save: a failing nullify raises with nothing
                // written, as Rails' rollback leaves it.
                format!(
                    "  def __nested_{name}_replace\n    unless {replaced}.nil?\n      old = {replaced}\n      {replaced} = nil\n{remove}    end\n  end\n\n  before_save :__nested_{name}_replace\n\n"
                )
            };
            replace_old + &format!(
                "  def __nested_{name}_save\n    unless {ivar}.nil?\n      children = {ivar}\n      {ivar} = nil\n      children.each do |child|\n        child.{foreign_key} = id\n{set_type}        child.save!\n      end\n    end\n{destroy_doomed}{reset}  end\n\n  after_save :__nested_{name}_save\n\n"
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
            let Some((names, allow_destroy)) = declaration(expr) else { continue };
            let Some(targets) = targets(model, &names, allow_destroy, &snapshot) else { continue };
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
