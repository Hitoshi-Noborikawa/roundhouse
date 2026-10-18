//! A model's class body that `extend`s one of the app's own modules and
//! then calls that module's methods (discourse's `SiteSetting`: `extend
//! SiteSettingExtension`, then `load_settings(…)`) runs the app's code as
//! the class loads; the methods it defines exist because it ran. Those
//! statements are marked ([`crate::expr::REPLAYED_CLASS_BODY`]) for the
//! Ruby family to replay where the source has them, among the class's
//! own methods: a framework or column method yields to what the macro
//! defines, and a `def` written after the call overrides it, as in Ruby.
//! The same shape on a plain class is already replayed
//! (`emit::ruby::library::app_extension_calls`).
//!
//! Only top-level statements are marked: a call inside a block or
//! condition (`plugins.each { |p| load_settings(p) }`) is not one the
//! replay can carry whole, and stays dropped and reported.

use std::collections::HashSet;

use crate::App;
use crate::dialect::{MethodReceiver, ModelBodyItem};
use crate::expr::ExprNode;

pub(super) fn mark(app: &mut App) {
    let modules: Vec<(String, HashSet<String>)> = app
        .library_classes
        .iter()
        .filter(|lc| lc.is_module && lc.origin.is_none())
        .map(|lc| {
            let methods = lc.methods.iter().filter(|m| m.receiver == MethodReceiver::Instance).map(|m| m.name.as_str().to_string()).collect();
            (lc.name.0.as_str().to_string(), methods)
        })
        .collect();
    for model in &mut app.models {
        let owner = model.name.0.as_str().to_string();
        let mut extended: Vec<usize> = Vec::new();
        for item in &mut model.body {
            let ModelBodyItem::Unknown { expr, .. } = item else { continue };
            let ExprNode::Send { recv: None, method, args, .. } = &*expr.node else { continue };
            if method.as_str() == "extend" {
                let found: Option<Vec<usize>> = args
                    .iter()
                    .map(|a| match &*a.node {
                        ExprNode::Const { path } => {
                            let written = path.iter().map(|s| s.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("::");
                            let nested = format!("{owner}::{written}");
                            modules.iter().position(|(n, _)| *n == nested).or_else(|| modules.iter().position(|(n, _)| *n == written))
                        }
                        _ => None,
                    })
                    .collect();
                if let Some(found) = found.filter(|f| !f.is_empty()) {
                    extended.extend(found);
                    expr.decisions |= crate::expr::REPLAYED_CLASS_BODY;
                }
                continue;
            }
            if extended.iter().any(|&i| modules[i].1.contains(method.as_str())) {
                expr.decisions |= crate::expr::REPLAYED_CLASS_BODY;
            }
        }
    }
}
