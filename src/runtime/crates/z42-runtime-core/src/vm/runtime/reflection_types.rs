//! Weak Type objects indexed by stable identity within one VM.
use crate::gc::{Heap, WeakRef};
use crate::object::{ObjRef, ReflectedTypeKey, Value};
use crate::vm::runtime::ids::tokens::TypeId;
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
pub struct ReflectionTypes {
    types: Mutex<FxHashMap<ReflectedTypeKey, CachedType>>,
    attributes: Mutex<FxHashMap<TypeId, Vec<WeakRef>>>,
}

impl ReflectionTypes {
    /// Called under the type publication lock for registered declarations.
    pub fn store_attributes(
        &self,
        heap: &Heap,
        ty: &Value,
        dependency: Option<TypeId>,
        attributes: Value,
    ) -> anyhow::Result<Value> {
        let mut watchers = self.attributes.lock();
        let Value::Object(object) = ty else {
            return Ok(attributes);
        };
        let Some(&slot) = object.type_desc().field_index.get("__attributeCache") else {
            return Ok(attributes);
        };
        let cached = object.borrow().field_value(slot);
        if matches!(cached, Value::Array(_)) {
            return Ok(cached);
        }
        let wrote = object.borrow().try_set_field_value(slot, &attributes)?;
        wrote.with_barrier_value(&attributes, |value| {
            heap.write_barrier_field(ty, slot, value)
        });
        if let Some(id) = dependency {
            if let Some(weak) = heap.make_weak(ty) {
                watchers.entry(id).or_default().push(weak);
            }
        }
        Ok(attributes)
    }

    /// The owning Type objects remain stable; only their dependent results clear.
    pub fn invalidate_attributes(&self, heap: &Heap, id: TypeId) {
        let watchers = self.attributes.lock().remove(&id).unwrap_or_default();
        for weak in watchers {
            let Some(ty @ Value::Object(object)) = heap.upgrade_weak(&weak) else {
                continue;
            };
            let Some(&slot) = object.type_desc().field_index.get("__attributeCache") else {
                continue;
            };
            if object.borrow().set_field_value(slot, &Value::Null) {
                heap.write_barrier_field(&ty, slot, &Value::Null);
            }
        }
    }

    pub fn get(
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

    pub fn publish(
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
