//! Source-level debug records and frame-name queries. No VM state or execution.

mod frames;
mod symbols;

pub use frames::{format_frame_name, for_each_frame_name_piece, frame_file};
pub use symbols::{DbugFuncEntry, LineEntry, LocalVar};
