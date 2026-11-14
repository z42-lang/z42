//! Value representation and primitive cell codecs.

mod value;
mod payloads;
mod codec;
mod cells;
pub mod ref_word;
pub mod tparam_cell;
pub mod struct_blob;
pub use value::*;
pub use payloads::*;
pub use codec::*;
pub use cells::Cells;

pub mod format;
pub mod boxed_hash;

mod kind;
pub use kind::value_kind_name;
