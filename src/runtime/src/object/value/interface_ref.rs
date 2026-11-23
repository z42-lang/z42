//! A managed reference to an object through a particular closed interface slot table.

use crate::gc::Heap;
use crate::object::{ArrRef, ArrayObj, ObjRef, Str, Value};

/// A source interface and its receiver share an immutable GC backing allocation.
/// Keeping a single handle in a register preserves the `Value` stride and lets
/// reference cells use their existing boxed-value path.
#[derive(Debug, Clone, Copy)]
pub struct InterfaceRef(ArrRef);

impl InterfaceRef {
    /// Create a reference with the source interface's exact slot prefix, e.g.
    /// `Demo.ISink<@Demo.Animal>`. The caller owns the interface type check,
    /// keeps both inputs rooted, and publishes the result before the next
    /// safepoint. The allocation must run with collection deferred to that
    /// safepoint. Returns `None` if the backing array cannot be allocated.
    pub fn new(heap: &Heap, receiver: ObjRef, source: Str) -> Option<Self> {
        Self::allocate(heap, Value::Object(receiver), source, Value::Null, Value::Null, Value::Null)
    }

    /// Capture the first source interface even when converting a projected reference again.
    pub(crate) fn project(heap: &Heap, receiver: Value, source: Str, legacy: Str, target: Str)
        -> Option<Self>
    {
        let (receiver, source, legacy, previous) = match receiver {
            Value::InterfaceRef(view) => (view.receiver(), view.source(), view.legacy(), receiver),
            value => (value, source, Value::Str(legacy), Value::Null),
        };
        Self::allocate(heap, receiver, source, legacy, Value::Str(target), previous)
    }

    fn allocate(
        heap: &Heap, receiver: Value, source: Str, legacy: Value, target: Value, previous: Value,
    ) -> Option<Self> {
        let values = [receiver, Value::Str(source), legacy, target, previous];
        match ArrayObj::alloc_frozen(heap, &values) {
            Value::Array(backing) => Some(Self(backing)),
            _ => None,
        }
    }

    pub fn receiver(self) -> Value {
        self.0.borrow().get(0).expect("interface reference has a receiver")
    }

    pub(crate) fn legacy(self) -> Value {
        self.0.borrow().get(2).expect("interface reference has a legacy slot")
    }

    pub fn matches_target(self, name: &str) -> bool {
        let mut view = self;
        loop {
            let backing = view.0.borrow();
            if matches!(backing.get(3), Some(Value::Str(target)) if target.as_str() == name) {
                return true;
            }
            match backing.get(4) {
                Some(Value::InterfaceRef(previous)) => view = previous,
                _ => return false,
            }
        }
    }

    pub(crate) fn source(self) -> Str {
        let Some(Value::Str(source)) = self.0.borrow().get(1) else {
            unreachable!("interface reference has a source interface")
        };
        source
    }

    pub(crate) fn backing(self) -> ArrRef {
        self.0
    }
}
