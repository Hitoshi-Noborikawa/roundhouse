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

    pub(super) fn derived_storage_class_value(
        &self,
        recv_ty: Option<&Ty>,
        method: &Symbol,
        args: &[Expr],
    ) -> Option<Ty> {
        if !matches!(
            method.as_str(),
            "new"
                | "superclass"
                | "ancestors"
                | "subclasses"
                | "method"
                | "public_method"
                | "singleton_method"
                | "freeze"
                | "itself"
                | "dup"
                | "clone"
        ) {
            return None;
        }
        let recv_ty = recv_ty?;
        let recv_ty = crate::analyze::singleton_of(Some(recv_ty)).unwrap_or(recv_ty);
        let Ty::Class { id, .. } = recv_ty else {
            return None;
        };
        if method.as_str() == "new" && id.0.as_str() != "Class" {
            return None;
        }
        if super::ClassInfo::source_class_method(self.classes(), id, method) {
            return None;
        }
        let singleton = |of| Ty::Class {
            id: ClassId(Symbol::from("Class")),
            args: vec![of].into(),
        };
        if id.0.as_str() == "Class" && method.as_str() == "new" && args.len() == 1 {
            if self
                .const_resolver
                .as_ref()
                .is_some_and(|r| r.has_source_namespace("Class"))
            {
                return None;
            }
            let parent = args[0].ty.as_ref()?;
            let parent = crate::analyze::singleton_of(Some(parent)).unwrap_or(parent);
            let has_storage_parent = match parent {
                Ty::Union { variants } => {
                    variants.iter().any(|ty| self.storage_lineage(ty).is_some())
                }
                ty => self.storage_lineage(ty).is_some(),
            };
            if !has_storage_parent {
                return None;
            }
            // An anonymous subclass is at least an instance of this nominal
            // class. Keep that upper bound through assignments and parameters.
            return Some(singleton(parent.clone()));
        }
        let lineage = self.storage_lineage(recv_ty)?;
        if matches!(
            method.as_str(),
            "method" | "public_method" | "singleton_method"
        ) {
            let [name] = args else { return None };
            let name = match &*name.node {
                ExprNode::Lit {
                    value: Literal::Sym { value },
                } => value.clone(),
                ExprNode::Lit {
                    value: Literal::Str { value },
                } => Symbol::from(value.as_str()),
                _ => return None,
            };
            let ret = self.derived_storage_class_value(Some(recv_ty), &name, &[])?;
            return Some(Ty::Fn {
                params: vec![].into(),
                block: None,
                ret: std::sync::Arc::new(ret),
                effects: crate::effect::EffectSet::default(),
            });
        }
        if !args.is_empty() {
            return None;
        }
        match method.as_str() {
            "superclass" => {
                let parent = self.classes().get(id)?.parent.as_ref()?;
                let parent = Ty::Class {
                    id: parent.clone(),
                    args: vec![].into(),
                };
                self.storage_lineage(&parent)?;
                Some(singleton(parent))
            }
            "ancestors" => {
                let mut variants = lineage.into_iter().map(singleton).collect::<Vec<_>>();
                // The registry does not enumerate Object/Kernel or every
                // runtime mixin. This is an element union, not an exact list.
                variants.push(Ty::Class {
                    id: ClassId(Symbol::from("Class")),
                    args: vec![].into(),
                });
                Some(Ty::Array {
                    elem: std::sync::Arc::new(Ty::Union {
                        variants: variants.into(),
                    }),
                })
            }
            "subclasses" if !self.classes().get(id)?.is_module => Some(Ty::Array {
                elem: std::sync::Arc::new(singleton(recv_ty.clone())),
            }),
            "freeze" | "itself" | "dup" | "clone" => Some(singleton(recv_ty.clone())),
            _ => None,
        }
    }

    fn storage_lineage(&self, mut ty: &Ty) -> Option<Vec<Ty>> {
        while let Some(of) = crate::analyze::singleton_of(Some(ty)) {
            ty = of;
        }
        let Ty::Class { id, .. } = ty else {
            return None;
        };
        let mut current = Some(id);
        let mut seen = std::collections::HashSet::new();
        let mut lineage = Vec::new();
        let mut storage_owner = false;
        while let Some(id) = current {
            if !seen.insert(id) {
                break;
            }
            storage_owner |= storage_owner_name(id).is_some();
            lineage.push(Ty::Class {
                id: id.clone(),
                args: vec![].into(),
            });
            current = self.classes().get(id).and_then(|c| c.parent.as_ref());
        }
        storage_owner.then_some(lineage)
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
            unsupported = unsupported.or_else(|| storage_owner_name(id));
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

fn storage_owner_name(id: &ClassId) -> Option<&'static str> {
    match id.0.as_str() {
        "ActiveStorage::Transformers" => Some("ActiveStorage::Transformers"),
        "ActiveStorage::Transformers::Transformer" => {
            Some("ActiveStorage::Transformers::Transformer")
        }
        "ActiveStorage::Transformers::ImageProcessingTransformer" => {
            Some("ActiveStorage::Transformers::ImageProcessingTransformer")
        }
        "ActiveStorage::Service::GCSService" => Some("ActiveStorage::Service::GCSService"),
        _ => None,
    }
}
