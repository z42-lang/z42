//! Stable reflected identities, separate from replaceable metadata descriptors.
use crate::object::TypeDesc;
use crate::metadata::ids::{tokens::TypeId, type_table::TypeTable};
use std::sync::{Arc, Weak};

/// Stable reflection identity. The shape contains no managed object references.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReflectedTypeKey {
    Definition(crate::metadata::ids::tokens::TypeId),
    Symbol(Arc<str>),
    Array(Option<Arc<ReflectedTypeKey>>),
    Constructed {
        definition: Arc<ReflectedTypeKey>,
        arguments: Arc<[ReflectedTypeKey]>,
    },
}

#[derive(Debug)]
pub struct ReflectedTypeShape {
    pub definition: Option<ReflectedTypeHandle>,
    pub key: ReflectedTypeKey,
}

/// Metadata ownership follows the declaration, never a same-named lookup.
#[derive(Debug, Clone)]
pub struct ReflectedTypeHandle {
    source: DeclarationSource,
}

#[derive(Debug, Clone)]
enum DeclarationSource {
    Published { id: TypeId, owner: Weak<TypeTable> },
    // Collectible-context and native declarations are not in the root table.
    Declaration(Arc<TypeDesc>),
}

impl ReflectedTypeHandle {
    pub fn new(owner: &Arc<TypeTable>, declaration: Arc<TypeDesc>) -> Self {
        if owner.get(declaration.id).is_some() {
            Self {
                source: DeclarationSource::Published {
                    id: declaration.id,
                    owner: Arc::downgrade(owner),
                },
            }
        } else {
            Self::declaration(declaration)
        }
    }

    pub fn declaration(declaration: Arc<TypeDesc>) -> Self {
        Self {
            source: DeclarationSource::Declaration(declaration),
        }
    }

    pub fn resolve(&self) -> Option<Arc<TypeDesc>> {
        match &self.source {
            DeclarationSource::Published { id, owner } => owner.upgrade()?.get_arc(*id),
            DeclarationSource::Declaration(td) => Some(td.clone()),
        }
    }

    pub fn key(&self) -> ReflectedTypeKey {
        match &self.source {
            DeclarationSource::Published { id, .. } => ReflectedTypeKey::Definition(*id),
            DeclarationSource::Declaration(td) if td.id == TypeId::UNRESOLVED => {
                ReflectedTypeKey::Symbol(td.name.as_str().into())
            }
            DeclarationSource::Declaration(td) => ReflectedTypeKey::Definition(td.id),
        }
    }
}
