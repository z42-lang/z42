//! Weak Type objects owned by one VM and one published metadata snapshot.
use crate::gc::{Heap, WeakRef};
use crate::object::{ObjRef, ReflectedTypeKey, Value};
use parking_lot::Mutex;
use rustc_hash::FxHashMap;

struct CachedType {
    object: WeakRef,
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
struct Snapshot {
    revision: i64,
    types: FxHashMap<ReflectedTypeKey, CachedType>,
}

#[derive(Default)]
pub(crate) struct ReflectionTypes {
    snapshot: Mutex<Snapshot>,
}

impl ReflectionTypes {
    pub(crate) fn get(
        &self,
        heap: &Heap,
        revision: i64,
        key: &ReflectedTypeKey,
        arguments: &[ObjRef],
    ) -> Option<Value> {
        if revision == 0 {
            return None;
        }
        let mut snapshot = self.snapshot.lock();
        if revision < snapshot.revision {
            return None;
        }
        if revision != snapshot.revision {
            snapshot.types.clear();
            snapshot.revision = revision;
        }
        let value = snapshot
            .types
            .get(key)
            .filter(|cached| cached.arguments_match(arguments))
            .and_then(|cached| heap.upgrade_weak(&cached.object));
        if value.is_none() {
            snapshot.types.remove(key);
        }
        value
    }

    pub(crate) fn publish(
        &self,
        heap: &Heap,
        revision: i64,
        key: ReflectedTypeKey,
        arguments: Vec<ObjRef>,
        value: Value,
    ) -> Value {
        if revision == 0 {
            return value;
        }
        let Some(weak) = heap.make_weak(&value) else {
            return value;
        };
        let mut snapshot = self.snapshot.lock();
        if revision < snapshot.revision {
            return value;
        }
        if revision != snapshot.revision {
            snapshot.types.clear();
            snapshot.revision = revision;
        }
        if let Some(existing) = snapshot
            .types
            .get(&key)
            .filter(|cached| cached.arguments_match(&arguments))
            .and_then(|cached| heap.upgrade_weak(&cached.object))
        {
            return existing;
        }
        snapshot.types.insert(
            key,
            CachedType {
                object: weak,
                arguments,
            },
        );
        value
    }
}
