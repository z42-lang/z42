//! Value representation and primitive cell codecs.

mod value;
mod value_aux;
mod codec;
mod cells;
pub mod ref_word;
pub(crate) mod tparam_cell;
pub(crate) mod struct_blob;
pub use value::*;
pub use value_aux::*;
pub use codec::*;
pub use cells::Cells;
