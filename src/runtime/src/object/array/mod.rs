//! Array storage, element access and native sorting.

mod array;
mod array_elems;
mod array_access;
mod array_sort;
mod union_elems;
pub use array::*;

pub use array_access::{packed_array_data, reference_array_data};
