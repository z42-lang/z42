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
        match ArrayObj::alloc_frozen(heap, &[Value::Object(receiver), Value::Str(source)]) {
            Value::Array(backing) => Some(Self(backing)),
            _ => None,
        }
    }

    pub fn receiver(self) -> ObjRef {
        let Some(Value::Object(receiver)) = self.0.borrow().get(0) else {
            unreachable!("interface reference has a receiver")
        };
        receiver
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
