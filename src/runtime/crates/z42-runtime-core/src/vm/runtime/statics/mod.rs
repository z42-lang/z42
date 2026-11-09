//! Static storage and initialization facts. Execution and waiting belong to exec/init.
mod cctor;
mod table;
pub use cctor::{CctorRegistry, CctorState, CCTOR_SENTINEL, MODULE_PSEUDO_TYPE,
    is_module_pseudo_type, module_owns_symbol, owner_class_of_static_field, owner_class_of_static_func};
pub use table::{StaticTable, StaticCell, StaticOwner};
