//! Weak Type objects indexed by stable identity within one VM.
use crate::gc::{Heap, WeakRef};
use crate::object::{ObjRef, ReflectedTypeKey, Value};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

struct CachedType {
    object: WeakRef,
    // Identity cookies only: comparison never dereferences these unrooted handles.
    arguments: Vec<ObjRef>,
}

impl CachedType {
    fn arguments_match(&self, arguments: &[ObjRef]) -> bool {
        self.arguments.len() == arguments.len()
            && self
                .arguments
                .iter()
                .zip(arguments)
                .all(|(a, b)| ObjRef::ptr_eq(a, b))
    }
}

#[derive(Default)]
pub(crate) struct ReflectionTypes {
    types: Mutex<FxHashMap<ReflectedTypeKey, CachedType>>,
}

impl ReflectionTypes {
    pub(crate) fn get(
        &self,
        heap: &Heap,
        key: &ReflectedTypeKey,
        arguments: &[ObjRef],
    ) -> Option<Value> {
        let mut entries = self.types.lock();
        let value = entries
            .get(key)
            .filter(|cached| cached.arguments_match(arguments))
            .and_then(|cached| heap.upgrade_weak(&cached.object));
        if value.is_none() {
            entries.remove(key);
        }
        value
    }

    pub(crate) fn publish(
        &self,
        heap: &Heap,
        key: ReflectedTypeKey,
        arguments: Vec<ObjRef>,
        value: Value,
    ) -> Value {
        let Some(weak) = heap.make_weak(&value) else {
            return value;
        };
        let mut entries = self.types.lock();
        if let Some(existing) = entries
            .get(&key)
            .filter(|cached| cached.arguments_match(&arguments))
            .and_then(|cached| heap.upgrade_weak(&cached.object))
        {
            return existing;
        }
        entries.insert(
            key,
            CachedType {
                object: weak,
                arguments,
            },
        );
        value
    }
}
