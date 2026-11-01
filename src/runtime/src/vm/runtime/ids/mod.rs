//! Runtime identities and stable, append-only lookup tables.
pub mod tokens;
pub mod seg_vec;
pub mod func_table;
pub mod type_table;
pub mod str_table;
pub use func_table::FuncTable;
pub use type_table::TypeTable;
pub use str_table::StrTable;
