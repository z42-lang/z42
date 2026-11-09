//! Value representation and primitive cell codecs.

mod value;
mod value_aux;
mod codec;
mod cells;
pub mod ref_word;
pub mod tparam_cell;
pub mod struct_blob;
pub use value::*;
pub use value_aux::*;
pub use codec::*;
pub use cells::Cells;

pub mod format;
pub mod boxed_hash;

mod kind;
pub use kind::value_kind_name;
