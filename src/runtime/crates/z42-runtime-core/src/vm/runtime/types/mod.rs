//! Type descriptor assembly and registry constraint validation.
mod registry;
mod constraints;
pub use registry::{build_type_registry, try_fixup_inheritance, unconverged_type_names};
pub use constraints::verify_constraints;

#[cfg(test)]
mod constraint_tests;
