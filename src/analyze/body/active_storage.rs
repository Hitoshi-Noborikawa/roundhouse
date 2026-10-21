//! Exception owners are class/module values, not implementations of Rails'
//! image-processing or cloud-service constructor contracts.

use crate::expr::{Expr, ExprNode, Literal};
use crate::ident::{ClassId, Symbol};
use crate::ty::Ty;

use super::BodyTyper;

impl BodyTyper<'_> {
    pub(super) fn unsupported_active_storage_constructor(
        &self,
        recv_ty: Option<&Ty>,
        method: &Symbol,
        args: &[Expr],
    ) -> Option<&'static str> {
        // Reflective calls and captured constructor methods reach the same
        // missing API. An opaque name cannot rule out `new`.
        if matches!(
            method.as_str(),
            "send"
                | "public_send"
                | "__send__"
                | "try"
                | "try!"
                | "method"
                | "public_method"
                | "singleton_method"
        ) {
            let (first, rest) = args.split_first()?;
            let name = match &*first.node {
                ExprNode::Lit {
                    value: Literal::Sym { value },
                } => value.clone(),
                ExprNode::Lit {
                    value: Literal::Str { value },
                } => Symbol::from(value.as_str()),
                _ => return self.unsupported_storage_owner(recv_ty?),
            };
            return self.unsupported_active_storage_constructor(recv_ty, &name, rest);
        }
        if method.as_str() != "new" {
            return None;
        }
        self.unsupported_storage_owner(recv_ty?)
    }

    fn unsupported_storage_owner(&self, ty: &Ty) -> Option<&'static str> {
        if let Some(of) = crate::analyze::singleton_of(Some(ty)) {
            return self.unsupported_storage_owner(of);
        }
        if let Ty::Union { variants } = ty {
            return variants
                .iter()
                .find_map(|ty| self.unsupported_storage_owner(ty));
        }
        let Ty::Class { id, .. } = ty else {
            return None;
        };
        let mut current = Some(id);
        let mut seen = std::collections::HashSet::new();
        let mut unsupported = None;
        while let Some(id) = current {
            if !seen.insert(id) {
                break;
            }
            let class = self.classes().get(id);
            // Reopening a namespace alone is insufficient. A real source
            // constructor (own, inherited or mixed in) supplies the contract.
            if class.is_some_and(|c| {
                c.source_class_methods.contains(&Symbol::from("new")) && !c.constructor_calls_super
            }) {
                return None;
            }
            if id.0.as_str() != "ActiveStorage::Transformers"
                && self.source_storage_initializer(id, &mut std::collections::HashSet::new())
            {
                return None;
            }
            unsupported = unsupported.or(match id.0.as_str() {
                "ActiveStorage::Transformers" => Some("ActiveStorage::Transformers"),
                "ActiveStorage::Transformers::Transformer" => {
                    Some("ActiveStorage::Transformers::Transformer")
                }
                "ActiveStorage::Transformers::ImageProcessingTransformer" => {
                    Some("ActiveStorage::Transformers::ImageProcessingTransformer")
                }
                "ActiveStorage::Service::GCSService" => Some("ActiveStorage::Service::GCSService"),
                _ => None,
            });
            current = class.and_then(|c| c.parent.as_ref());
        }
        unsupported
    }

    fn source_storage_initializer<'a>(
        &'a self,
        id: &'a ClassId,
        seen: &mut std::collections::HashSet<&'a ClassId>,
    ) -> bool {
        if !seen.insert(id) {
            return false;
        }
        self.classes().get(id).is_some_and(|c| {
            c.implements_initializer
                || c.includes
                    .iter()
                    .any(|m| self.source_storage_initializer(m, seen))
        })
    }
}
