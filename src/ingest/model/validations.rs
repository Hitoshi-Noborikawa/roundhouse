//! `validates` / `validates_*_of` option parsing.
//!
//! Recognized rules become `Validation` items. An option this build
//! does not lower is a warning and a survey gap, and the call is not
//! turned into a partial check.

use ruby_prism::Node;

use crate::Symbol;
use crate::dialect::{Comment, ModelBodyItem};
use crate::expr::{Expr, Literal};
use crate::span::Span;

use super::super::IngestError;
use super::super::util::{string_value, symbol_value};

pub(super) fn validation_items(
    parsed: Vec<crate::dialect::Validation>,
    leading_comments: Vec<Comment>,
    span: Span,
) -> Vec<ModelBodyItem> {
    parsed
        .into_iter()
        .enumerate()
        .map(|(i, validation)| ModelBodyItem::Validation {
            validation,
            // The declaration's comments belong to the first of the
            // items it expands to.
            leading_comments: if i == 0 {
                leading_comments.clone()
            } else {
                Vec::new()
            },
            leading_blank_line: false,
            span,
        })
        .collect()
}

/// Which `validates` spelling `parse_validates_call` is reading.
#[derive(Clone, Copy)]
pub(super) enum ValidatesKind {
    /// `validates :attr, presence: true, numericality: { … }`.
    Map,
    Numericality,
    Inclusion,
    Format,
}

pub(super) enum ParsedValidates {
    /// Every option on the call is lowered. Empty rules are a real
    /// no-op (`presence: false`), not a dropped validator.
    Modeled(Vec<crate::dialect::Validation>),
    /// An option this build does not lower. The call is not a
    /// `Validation` and not an `Unknown` re-warning: the other rules
    /// must not run while this one vanishes, and a condition must not
    /// become an unconditional check.
    Gap,
    /// Not a shape this parser claims (`validates` with a block). The
    /// call stays on the unsupported-DSL ledger.
    Decline,
}

struct BuiltRule {
    rule: crate::dialect::ValidationRule,
    if_cond: Option<Expr>,
    unless_cond: Option<Expr>,
}

impl BuiltRule {
    fn bare(rule: crate::dialect::ValidationRule) -> Self {
        Self { rule, if_cond: None, unless_cond: None }
    }
}

enum RuleParse {
    /// `presence: false` — Rails adds no validator.
    Skip,
    Rule(BuiltRule),
    Gap(Vec<String>),
}

pub(super) fn parse_validates_call(
    call: &ruby_prism::CallNode<'_>,
    file: &str,
    kind: ValidatesKind,
    class_consts: &super::ClassConsts,
) -> ParsedValidates {
    use crate::dialect::{Validation, ValidationRule};
    if call.block().is_some() {
        return ParsedValidates::Decline;
    }
    let Some(args) = call.arguments() else {
        return ParsedValidates::Decline;
    };
    let span = Span {
        file: super::super::sources::file_id(file),
        start: call.location().start_offset() as u32,
        end: call.location().end_offset() as u32,
    };
    let method = match kind {
        ValidatesKind::Map => "validates",
        ValidatesKind::Numericality => "validates_numericality_of",
        ValidatesKind::Inclusion => "validates_inclusion_of",
        ValidatesKind::Format => "validates_format_of",
    };

    let mut attrs: Vec<Symbol> = Vec::new();
    let mut option_node: Option<Node<'_>> = None;
    let mut gaps: Vec<String> = Vec::new();
    for arg in args.arguments().iter() {
        if let Some(name) = super::super::util::symbol_or_string_value(&arg) {
            attrs.push(Symbol::from(name.as_str()));
            continue;
        }
        if hash_elements(&arg).is_some() {
            if option_node.is_some() {
                gaps.push("options".to_string());
            } else {
                option_node = Some(arg);
            }
            continue;
        }
        gaps.push("argument".to_string());
    }

    let mut allow_blank = false;
    let mut allow_nil: Option<bool> = None;
    let mut call_if_cond: Option<Expr> = None;
    let mut call_unless_cond: Option<Expr> = None;
    let mut drafts: Vec<BuiltRule> = Vec::new();

    if let Some(node) = &option_node {
        let elements = hash_elements(node).unwrap();
        match kind {
            ValidatesKind::Map => {
                for el in elements.iter() {
                    let Some(assoc) = el.as_assoc_node() else {
                        gaps.push("options".to_string());
                        continue;
                    };
                    let Some(key) = symbol_value(&assoc.key()) else {
                        gaps.push("options".to_string());
                        continue;
                    };
                    let value = assoc.value();
                    match key.as_str() {
                        "allow_blank" => match super::super::util::bool_value(&value) {
                            Some(true) => allow_blank = true,
                            Some(false) => {}
                            None => gaps.push("allow_blank".to_string()),
                        },
                        "allow_nil" => match super::super::util::bool_value(&value) {
                            Some(flag) => allow_nil = Some(flag),
                            None => gaps.push("allow_nil".to_string()),
                        },
                        "if" => push_condition(&value, file, &mut call_if_cond, "if", &mut gaps),
                        "unless" => {
                            push_condition(&value, file, &mut call_unless_cond, "unless", &mut gaps)
                        }
                        "on" | "strict" | "message" => gaps.push(key),
                        other => match parse_validator(other, &value, file, class_consts) {
                            RuleParse::Skip => {}
                            RuleParse::Rule(built) => drafts.push(built),
                            RuleParse::Gap(more) => gaps.extend(more),
                        },
                    }
                }
            }
            ValidatesKind::Numericality | ValidatesKind::Inclusion | ValidatesKind::Format => {
                parse_of_options(
                    kind,
                    &elements,
                    file,
                    &mut allow_blank,
                    &mut allow_nil,
                    &mut call_if_cond,
                    &mut call_unless_cond,
                    &mut drafts,
                    &mut gaps,
                    class_consts,
                );
            }
        }
    } else if matches!(kind, ValidatesKind::Numericality) && gaps.is_empty() && !attrs.is_empty() {
        // `validates_numericality_of :price` is `numericality: true`.
        drafts.push(BuiltRule::bare(ValidationRule::Numericality {
            only_integer: false,
            gt: None,
            lt: None,
            gte: None,
            lte: None,
        }));
    } else if !matches!(kind, ValidatesKind::Map) && !attrs.is_empty() {
        gaps.push(match kind {
            ValidatesKind::Inclusion => "inclusion".to_string(),
            ValidatesKind::Format => "format".to_string(),
            ValidatesKind::Numericality | ValidatesKind::Map => "options".to_string(),
        });
    }

    if allow_blank && drafts.iter().any(|draft| !blank_safe(&draft.rule)) {
        gaps.push("allow_blank".to_string());
    }
    // Call-level `allow_nil: true` is Rails' skip-when-nil for every
    // validator in the call. Length stores it. The other rules this
    // build lowers reject nil, so applying them would reject records
    // Rails accepts.
    if allow_nil == Some(true)
        && drafts
            .iter()
            .any(|draft| !matches!(draft.rule, ValidationRule::Length { .. }))
    {
        gaps.push("allow_nil".to_string());
    }

    if !gaps.is_empty() {
        gaps.sort();
        gaps.dedup();
        record_validation_gaps(file, span, method, &attrs, &gaps);
        return ParsedValidates::Gap;
    }
    if attrs.is_empty() {
        return ParsedValidates::Decline;
    }

    if allow_blank {
        drafts.retain(|draft| !matches!(draft.rule, ValidationRule::Presence));
    }
    if let Some(flag) = allow_nil {
        for draft in &mut drafts {
            if let ValidationRule::Length {
                allow_nil: slot, ..
            } = &mut draft.rule
            {
                if slot.is_none() {
                    *slot = Some(flag);
                }
            }
        }
    }

    let mut out = Vec::new();
    for attr in &attrs {
        if drafts.is_empty() {
            out.push(Validation {
                attribute: attr.clone(),
                rules: Vec::new(),
                if_cond: call_if_cond.clone(),
                unless_cond: call_unless_cond.clone(),
            });
            continue;
        }
        let mut grouped: Option<Validation> = None;
        for draft in &drafts {
            // A condition inside the rule hash replaces the call-level
            // one for that key — the other key still merges in.
            let if_cond = draft.if_cond.clone().or_else(|| call_if_cond.clone());
            let unless_cond = draft.unless_cond.clone().or_else(|| call_unless_cond.clone());
            if let Some(current) = grouped.as_mut() {
                if current.if_cond == if_cond && current.unless_cond == unless_cond {
                    current.rules.push(draft.rule.clone());
                    continue;
                }
                out.push(grouped.take().unwrap());
            }
            grouped = Some(Validation {
                attribute: attr.clone(),
                rules: vec![draft.rule.clone()],
                if_cond,
                unless_cond,
            });
        }
        if let Some(validation) = grouped {
            out.push(validation);
        }
    }
    ParsedValidates::Modeled(out)
}

/// `if:` / `unless:` — the same reader a callback uses. A Symbol is the
/// predicate called on the record; a zero-arity lambda/proc contributes
/// its body. A later value for the same key replaces the earlier one,
/// matching Ruby's last-duplicate-key-wins rule. Anything the reader
/// rejects (an arity>0 lambda, a String, an unsupported array, a
/// captured local, a body survey mode had to stub with `nil`) stays a
/// reported gap.
fn push_condition(
    value: &Node<'_>,
    file: &str,
    cond_slot: &mut Option<Expr>,
    option: &str,
    gaps: &mut Vec<String>,
) {
    // Survey mode turns an unsupported expression into `nil` and still
    // returns it. A guard built from that placeholder skips or runs
    // checks Rails would not. The expression error stays on the survey
    // ledger; the validates call is the reported gap.
    let before = super::super::survey::recorded().len();
    if let Some(cond) = super::callback_condition(value, file)
        && super::super::survey::recorded().len() == before
    {
        *cond_slot = Some(cond);
        return;
    }
    gaps.push(option.to_string());
}

/// `allow_blank: true` skips every validator when the value is blank.
/// Presence is then a dead check (Rails runs validations before
/// `before_save`, which is what lobsters' `allow_blank: true, presence:
/// true` session token relies on). A max-only length passes on blank,
/// and absence wants blank. Anything else needs a skip-when-blank guard
/// this build does not lower.
fn blank_safe(rule: &crate::dialect::ValidationRule) -> bool {
    use crate::dialect::ValidationRule;
    match rule {
        ValidationRule::Presence | ValidationRule::Absence => true,
        ValidationRule::Length { min: None, .. } => true,
        _ => false,
    }
}

fn record_validation_gaps(
    file: &str,
    span: Span,
    method: &str,
    attrs: &[Symbol],
    options: &[String],
) {
    let targets: Vec<(String, String)> = if attrs.is_empty() {
        options
            .iter()
            .map(|option| (format!("{method} {option}:"), option.clone()))
            .collect()
    } else {
        attrs
            .iter()
            .flat_map(|attr| {
                options
                    .iter()
                    .map(move |option| (format!("{method} :{attr}, {option}:"), option.clone()))
            })
            .collect()
    };
    for (construct, option) in targets {
        let detail = if matches!(
            option.as_str(),
            "if" | "unless" | "on" | "allow_nil" | "allow_blank"
        ) || option.ends_with(": if")
            || option.ends_with(": unless")
        {
            "condition not lowered; emitting the check would reject records Rails accepts"
        } else {
            "validation option not lowered; Rails rejects records this build saves"
        };
        let mut diagnostic =
            crate::diagnostic::Diagnostic::unsupported(span, None, construct, detail);
        diagnostic.severity = crate::diagnostic::Severity::Warning;
        let message = diagnostic.message.clone();
        super::super::prism::push(diagnostic);
        super::super::survey::record(&IngestError::Unsupported {
            file: file.into(),
            message,
        });
    }
}

fn hash_elements<'a>(node: &Node<'a>) -> Option<ruby_prism::NodeList<'a>> {
    if let Some(hash) = node.as_hash_node() {
        return Some(hash.elements());
    }
    node.as_keyword_hash_node().map(|hash| hash.elements())
}

fn parse_of_options<'a>(
    kind: ValidatesKind,
    elements: &ruby_prism::NodeList<'a>,
    file: &str,
    allow_blank: &mut bool,
    allow_nil: &mut Option<bool>,
    call_if_cond: &mut Option<Expr>,
    call_unless_cond: &mut Option<Expr>,
    drafts: &mut Vec<BuiltRule>,
    gaps: &mut Vec<String>,
    class_consts: &super::ClassConsts,
) {
    use crate::dialect::ValidationRule;
    let mut only_integer = false;
    let mut gt: Option<f64> = None;
    let mut lt: Option<f64> = None;
    let mut gte: Option<f64> = None;
    let mut lte: Option<f64> = None;
    let mut inclusion: Option<InclusionSpec> = None;
    let mut pattern: Option<(String, String)> = None;
    let mut saw_rule_key = false;
    for el in elements.iter() {
        let Some(assoc) = el.as_assoc_node() else {
            gaps.push("options".to_string());
            continue;
        };
        let Some(key) = symbol_value(&assoc.key()) else {
            gaps.push("options".to_string());
            continue;
        };
        let value = assoc.value();
        match key.as_str() {
            "allow_blank" => match super::super::util::bool_value(&value) {
                Some(true) => *allow_blank = true,
                Some(false) => {}
                None => gaps.push("allow_blank".to_string()),
            },
            "allow_nil" => match super::super::util::bool_value(&value) {
                Some(flag) => *allow_nil = Some(flag),
                None => gaps.push("allow_nil".to_string()),
            },
            "if" => push_condition(&value, file, call_if_cond, "if", gaps),
            "unless" => push_condition(&value, file, call_unless_cond, "unless", gaps),
            "on" | "strict" | "message" => gaps.push(key),
            "only_integer" if matches!(kind, ValidatesKind::Numericality) => {
                saw_rule_key = true;
                match super::super::util::bool_value(&value) {
                    Some(flag) => only_integer = flag,
                    None => gaps.push("numericality: only_integer".to_string()),
                }
            }
            "greater_than" if matches!(kind, ValidatesKind::Numericality) => {
                saw_rule_key = true;
                match bound_value(&value, class_consts) {
                    Some(n) => gt = Some(n),
                    None => gaps.push("numericality: greater_than".to_string()),
                }
            }
            "less_than" if matches!(kind, ValidatesKind::Numericality) => {
                saw_rule_key = true;
                match bound_value(&value, class_consts) {
                    Some(n) => lt = Some(n),
                    None => gaps.push("numericality: less_than".to_string()),
                }
            }
            "greater_than_or_equal_to" if matches!(kind, ValidatesKind::Numericality) => {
                saw_rule_key = true;
                match bound_value(&value, class_consts) {
                    Some(n) => gte = Some(n),
                    None => gaps.push("numericality: greater_than_or_equal_to".to_string()),
                }
            }
            "less_than_or_equal_to" if matches!(kind, ValidatesKind::Numericality) => {
                saw_rule_key = true;
                match bound_value(&value, class_consts) {
                    Some(n) => lte = Some(n),
                    None => gaps.push("numericality: less_than_or_equal_to".to_string()),
                }
            }
            "equal_to" | "other_than" if matches!(kind, ValidatesKind::Numericality) => {
                saw_rule_key = true;
                gaps.push(format!("numericality: {key}"));
            }
            "in" | "within" if matches!(kind, ValidatesKind::Inclusion) => {
                saw_rule_key = true;
                match inclusion_values(&value) {
                    Some(spec) => inclusion = Some(spec),
                    None => gaps.push("inclusion: in".to_string()),
                }
            }
            "with" if matches!(kind, ValidatesKind::Format) => {
                saw_rule_key = true;
                match resolve_format_regex(&value, class_consts) {
                    Some(resolved) => pattern = Some(resolved),
                    None => gaps.push("format: with".to_string()),
                }
            }
            other => {
                let prefix = match kind {
                    ValidatesKind::Numericality => "numericality",
                    ValidatesKind::Inclusion => "inclusion",
                    ValidatesKind::Format => "format",
                    ValidatesKind::Map => "validates",
                };
                gaps.push(format!("{prefix}: {other}"));
            }
        }
    }
    if !gaps.is_empty() {
        return;
    }
    let rule = match kind {
        ValidatesKind::Numericality => ValidationRule::Numericality {
            only_integer,
            gt,
            lt,
            gte,
            lte,
        },
        ValidatesKind::Inclusion => match inclusion {
            Some(spec) => inclusion_rule(spec),
            None if !saw_rule_key => {
                gaps.push("inclusion".to_string());
                return;
            }
            None => {
                gaps.push("inclusion: in".to_string());
                return;
            }
        },
        ValidatesKind::Format => match pattern {
            Some((pattern, flags)) => ValidationRule::Format { pattern, flags },
            None => {
                gaps.push("format: with".to_string());
                return;
            }
        },
        ValidatesKind::Map => return,
    };
    drafts.push(BuiltRule::bare(rule));
}

fn parse_validator(key: &str, value: &Node<'_>, file: &str, class_consts: &super::ClassConsts) -> RuleParse {
    use crate::dialect::ValidationRule;
    match key {
        "presence" => parse_bool_or_hash(value, ValidationRule::Presence, "presence", file),
        "absence" => parse_bool_or_hash(value, ValidationRule::Absence, "absence", file),
        "length" => parse_length_rule(value, file, class_consts),
        "numericality" => parse_numericality(value, file, class_consts),
        "inclusion" => parse_inclusion(value, file),
        "format" => parse_format(value, file, class_consts),
        other => RuleParse::Gap(vec![other.to_string()]),
    }
}

/// `presence: true`, `presence: false`, or `presence: { if: :live? }`.
fn parse_bool_or_hash(
    value: &Node<'_>,
    rule: crate::dialect::ValidationRule,
    name: &str,
    file: &str,
) -> RuleParse {
    match super::super::util::bool_value(value) {
        Some(true) => return RuleParse::Rule(BuiltRule::bare(rule)),
        Some(false) => return RuleParse::Skip,
        None => {}
    }
    let Some(elements) = hash_elements(value) else {
        return RuleParse::Gap(vec![name.to_string()]);
    };
    let mut built = BuiltRule::bare(rule);
    let mut gaps = Vec::new();
    for el in elements.iter() {
        let Some(assoc) = el.as_assoc_node() else {
            gaps.push(name.to_string());
            continue;
        };
        let Some(key) = symbol_value(&assoc.key()) else {
            gaps.push(name.to_string());
            continue;
        };
        let child = assoc.value();
        match key.as_str() {
            "if" => push_condition(&child, file, &mut built.if_cond, "if", &mut gaps),
            "unless" => {
                push_condition(&child, file, &mut built.unless_cond, "unless", &mut gaps)
            }
            "allow_nil" => match super::super::util::bool_value(&child) {
                Some(false) => {}
                _ => gaps.push("allow_nil".to_string()),
            },
            other => gaps.push(format!("{name}: {other}")),
        }
    }
    if gaps.is_empty() {
        RuleParse::Rule(built)
    } else {
        RuleParse::Gap(gaps)
    }
}

fn parse_numericality(value: &Node<'_>, file: &str, class_consts: &super::ClassConsts) -> RuleParse {
    use crate::dialect::ValidationRule;
    if super::super::util::bool_value(value) == Some(true) {
        return RuleParse::Rule(BuiltRule::bare(ValidationRule::Numericality {
            only_integer: false,
            gt: None,
            lt: None,
            gte: None,
            lte: None,
        }));
    }
    if super::super::util::bool_value(value) == Some(false) {
        return RuleParse::Skip;
    }
    let Some(elements) = hash_elements(value) else {
        return RuleParse::Gap(vec!["numericality".to_string()]);
    };
    let mut only_integer = false;
    let mut gt = None;
    let mut lt = None;
    let mut gte = None;
    let mut lte = None;
    let mut built = BuiltRule::bare(ValidationRule::Numericality {
        only_integer: false,
        gt: None,
        lt: None,
        gte: None,
        lte: None,
    });
    let mut gaps = Vec::new();
    for el in elements.iter() {
        let Some(assoc) = el.as_assoc_node() else {
            gaps.push("numericality".to_string());
            continue;
        };
        let Some(key) = symbol_value(&assoc.key()) else {
            gaps.push("numericality".to_string());
            continue;
        };
        let child = assoc.value();
        match key.as_str() {
            "only_integer" => match super::super::util::bool_value(&child) {
                Some(flag) => only_integer = flag,
                None => gaps.push("numericality: only_integer".to_string()),
            },
            "greater_than" => match bound_value(&child, class_consts) {
                Some(n) => gt = Some(n),
                None => gaps.push("numericality: greater_than".to_string()),
            },
            "less_than" => match bound_value(&child, class_consts) {
                Some(n) => lt = Some(n),
                None => gaps.push("numericality: less_than".to_string()),
            },
            "greater_than_or_equal_to" => match bound_value(&child, class_consts) {
                Some(n) => gte = Some(n),
                None => gaps.push("numericality: greater_than_or_equal_to".to_string()),
            },
            "less_than_or_equal_to" => match bound_value(&child, class_consts) {
                Some(n) => lte = Some(n),
                None => gaps.push("numericality: less_than_or_equal_to".to_string()),
            },
            "equal_to" | "other_than" => gaps.push(format!("numericality: {key}")),
            "if" => push_condition(&child, file, &mut built.if_cond, "if", &mut gaps),
            "unless" => {
                push_condition(&child, file, &mut built.unless_cond, "unless", &mut gaps)
            }
            "allow_nil" => match super::super::util::bool_value(&child) {
                Some(false) => {}
                _ => gaps.push("allow_nil".to_string()),
            },
            other => gaps.push(format!("numericality: {other}")),
        }
    }
    if !gaps.is_empty() {
        return RuleParse::Gap(gaps);
    }
    built.rule = ValidationRule::Numericality {
        only_integer,
        gt,
        lt,
        gte,
        lte,
    };
    RuleParse::Rule(built)
}

fn parse_inclusion(value: &Node<'_>, file: &str) -> RuleParse {
    if let Some(spec) = inclusion_values(value) {
        return RuleParse::Rule(BuiltRule::bare(inclusion_rule(spec)));
    }
    let Some(elements) = hash_elements(value) else {
        return RuleParse::Gap(vec!["inclusion".to_string()]);
    };
    let mut values = None;
    let mut built = BuiltRule::bare(inclusion_rule(InclusionSpec::List(Vec::new())));
    let mut gaps = Vec::new();
    for el in elements.iter() {
        let Some(assoc) = el.as_assoc_node() else {
            gaps.push("inclusion".to_string());
            continue;
        };
        let Some(key) = symbol_value(&assoc.key()) else {
            gaps.push("inclusion".to_string());
            continue;
        };
        let child = assoc.value();
        match key.as_str() {
            "in" | "within" => match inclusion_values(&child) {
                Some(spec) => values = Some(spec),
                None => gaps.push("inclusion: in".to_string()),
            },
            "if" => push_condition(&child, file, &mut built.if_cond, "if", &mut gaps),
            "unless" => {
                push_condition(&child, file, &mut built.unless_cond, "unless", &mut gaps)
            }
            "allow_nil" => match super::super::util::bool_value(&child) {
                Some(false) => {}
                _ => gaps.push("allow_nil".to_string()),
            },
            other => gaps.push(format!("inclusion: {other}")),
        }
    }
    if !gaps.is_empty() {
        return RuleParse::Gap(gaps);
    }
    match values {
        Some(spec) => {
            built.rule = inclusion_rule(spec);
            RuleParse::Rule(built)
        }
        None => RuleParse::Gap(vec!["inclusion: in".to_string()]),
    }
}

fn parse_format(value: &Node<'_>, file: &str, class_consts: &super::ClassConsts) -> RuleParse {
    use crate::dialect::ValidationRule;
    if let Some((pattern, flags)) = resolve_format_regex(value, class_consts) {
        return RuleParse::Rule(BuiltRule::bare(ValidationRule::Format { pattern, flags }));
    }
    if value.as_regular_expression_node().is_some()
        || value.as_interpolated_regular_expression_node().is_some()
    {
        // A flagged literal with a flag no target can reproduce exactly
        // (`/a/x`), or an interpolation this build can't flatten, is not
        // the pattern the lowerer emits.
        return RuleParse::Gap(vec!["format: with".to_string()]);
    }
    let Some(elements) = hash_elements(value) else {
        return RuleParse::Gap(vec!["format".to_string()]);
    };
    let mut pattern: Option<(String, String)> = None;
    let mut built = BuiltRule::bare(ValidationRule::Format {
        pattern: String::new(),
        flags: String::new(),
    });
    let mut gaps = Vec::new();
    for el in elements.iter() {
        let Some(assoc) = el.as_assoc_node() else {
            gaps.push("format".to_string());
            continue;
        };
        let Some(key) = symbol_value(&assoc.key()) else {
            gaps.push("format".to_string());
            continue;
        };
        let child = assoc.value();
        match key.as_str() {
            "with" => match resolve_format_regex(&child, class_consts) {
                Some(resolved) => pattern = Some(resolved),
                None => gaps.push("format: with".to_string()),
            },
            "if" => push_condition(&child, file, &mut built.if_cond, "if", &mut gaps),
            "unless" => {
                push_condition(&child, file, &mut built.unless_cond, "unless", &mut gaps)
            }
            "allow_nil" => match super::super::util::bool_value(&child) {
                Some(false) => {}
                _ => gaps.push("allow_nil".to_string()),
            },
            other => gaps.push(format!("format: {other}")),
        }
    }
    if !gaps.is_empty() {
        return RuleParse::Gap(gaps);
    }
    match pattern {
        Some((pattern, flags)) => {
            built.rule = ValidationRule::Format { pattern, flags };
            RuleParse::Rule(built)
        }
        None => RuleParse::Gap(vec!["format: with".to_string()]),
    }
}

/// Resolve a `format: { with: … }` value (or a bare `format: /…/`
/// shorthand) to the regex literal it names, restricted to the flag
/// combinations every emit target this build ships can reproduce with
/// Ruby's exact meaning: no flags, `i` (case-insensitive) alone, `m`
/// (Ruby's dot-matches-newline) alone, or both together. `x`
/// (extended) has no faithful JS equivalent and `o` / an encoding flag
/// changes match semantics in ways no target mirrors, so either one
/// stays a gap rather than silently dropping. The value can be a
/// direct `/pattern/flags` literal, a model constant assigned one
/// (`NAME_RE`, `.freeze`d or aliased), or a constant assigned an
/// interpolation of exactly one other such constant whose flags agree
/// (`ClassConsts::regex` flattens that case).
fn resolve_format_regex(
    node: &Node<'_>,
    class_consts: &super::ClassConsts,
) -> Option<(String, String)> {
    let (pattern, flags) = class_consts.regex(node)?;
    if flags.chars().all(|c| matches!(c, 'i' | 'm')) {
        Some((pattern, flags))
    } else {
        None
    }
}

/// `length: { minimum: N, maximum: M }`. `is:`, `in:`, and `within:`
/// change which lengths Rails accepts; they are a gap, not a silent
/// drop beside a bound this build does lower.
fn parse_length_rule(value: &ruby_prism::Node<'_>, file: &str, class_consts: &super::ClassConsts) -> RuleParse {
    use crate::dialect::ValidationRule;
    let Some(elements) = hash_elements(value) else {
        return RuleParse::Gap(vec!["length".to_string()]);
    };
    let mut min: Option<u32> = None;
    let mut max: Option<u32> = None;
    let mut allow_nil: Option<bool> = None;
    let mut message: Option<String> = None;
    let mut built = BuiltRule::bare(ValidationRule::Length {
        min: None,
        max: None,
        message: None,
        allow_nil: None,
    });
    let mut gaps = Vec::new();
    for el in elements.iter() {
        let Some(assoc) = el.as_assoc_node() else {
            gaps.push("length".to_string());
            continue;
        };
        let Some(key) = symbol_value(&assoc.key()) else {
            gaps.push("length".to_string());
            continue;
        };
        let child = assoc.value();
        match key.as_str() {
            "minimum" | "maximum" => match class_consts.integer(&child) {
                Some(n) if n >= 0 => {
                    let n = n as u32;
                    if key.as_str() == "minimum" {
                        min = Some(n);
                    } else {
                        max = Some(n);
                    }
                }
                _ => gaps.push(format!("length: {key}")),
            },
            "allow_nil" => match super::super::util::bool_value(&child) {
                Some(flag) => allow_nil = Some(flag),
                None => gaps.push("length: allow_nil".to_string()),
            },
            "message" => match string_value(&child) {
                Some(text) => message = Some(text),
                None => gaps.push("length: message".to_string()),
            },
            "if" => push_condition(&child, file, &mut built.if_cond, "if", &mut gaps),
            "unless" => {
                push_condition(&child, file, &mut built.unless_cond, "unless", &mut gaps)
            }
            "is" | "in" | "within" => gaps.push(format!("length: {key}")),
            other => gaps.push(format!("length: {other}")),
        }
    }
    if !gaps.is_empty() {
        return RuleParse::Gap(gaps);
    }
    if min.is_none() && max.is_none() {
        return RuleParse::Gap(vec!["length".to_string()]);
    }
    built.rule = ValidationRule::Length {
        min,
        max,
        message,
        allow_nil,
    };
    RuleParse::Rule(built)
}

/// A numericality bound: a number literal, or an integer constant of the
/// class (`less_than_or_equal_to: MAX_SCORE`).
fn bound_value(node: &Node<'_>, class_consts: &super::ClassConsts) -> Option<f64> {
    if let Some(n) = class_consts.integer(node) {
        return Some(n as f64);
    }
    number_value(node)
}

fn number_value(node: &Node<'_>) -> Option<f64> {
    if let Some(n) = super::super::util::integer_value(node) {
        return Some(n as f64);
    }
    node.as_float_node().map(|float| float.value())
}

enum InclusionSpec {
    List(Vec<Literal>),
    Range(crate::dialect::InclusionRange),
}

fn inclusion_rule(spec: InclusionSpec) -> crate::dialect::ValidationRule {
    use crate::dialect::ValidationRule;
    match spec {
        InclusionSpec::List(values) => ValidationRule::Inclusion { values, range: None },
        InclusionSpec::Range(range) => {
            ValidationRule::Inclusion { values: Vec::new(), range: Some(range) }
        }
    }
}

/// An array is list membership. A numeric range stays a range:
/// `Range#include?` covers the numbers between the endpoints, which a
/// list of the integers in between does not.
fn inclusion_values(node: &Node<'_>) -> Option<InclusionSpec> {
    if let Some(array) = node.as_array_node() {
        let mut out = Vec::new();
        for element in array.elements().iter() {
            out.push(literal_atom(&element)?);
        }
        return Some(InclusionSpec::List(out));
    }
    let range = node.as_range_node()?;
    let left = number_value(&range.left()?)?;
    let right = number_value(&range.right()?)?;
    Some(InclusionSpec::Range(crate::dialect::InclusionRange {
        begin: left,
        end: right,
        exclusive: range.is_exclude_end(),
    }))
}

fn literal_atom(node: &Node<'_>) -> Option<Literal> {
    if let Some(text) = string_value(node) {
        return Some(Literal::Str { value: text });
    }
    if let Some(text) = symbol_value(node) {
        return Some(Literal::Sym {
            value: Symbol::from(text.as_str()),
        });
    }
    if let Some(n) = super::super::util::integer_value(node) {
        return Some(Literal::Int { value: n });
    }
    if let Some(float) = node.as_float_node() {
        return Some(Literal::Float {
            value: float.value(),
        });
    }
    if let Some(flag) = super::super::util::bool_value(node) {
        return Some(Literal::Bool { value: flag });
    }
    node.as_nil_node().is_some().then_some(Literal::Nil)
}

