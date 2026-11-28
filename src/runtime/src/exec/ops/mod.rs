//! Engine-independent instruction operations.
pub mod object;
pub mod scalar;
pub mod structs;
pub(crate) mod union_frame;
pub(crate) mod union_heap;
pub mod vcall;
pub mod obj_new;
pub mod cast;
pub mod call;
pub mod stringify;
pub mod references;
pub mod const_string;
