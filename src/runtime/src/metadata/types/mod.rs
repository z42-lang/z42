//! Type descriptor assembly and registry constraint validation.
mod registry;
mod constraints;
pub use registry::{build_type_registry, try_fixup_inheritance, unconverged_type_names};
pub use constraints::verify_constraints;

#[cfg(all(test, z42_test_facade))]
mod constraint_tests;

/// Find the precise descriptor first. An imported instantiation can carry its
/// arguments separately while sharing the declaring package's definition.
pub(crate) fn resolve_base_instance(
    name: &str,
    resolve: impl Fn(&str) -> Option<std::sync::Arc<crate::object::TypeDesc>>,
) -> Option<(std::sync::Arc<crate::object::TypeDesc>, Box<[String]>)> {
    let definition = crate::zpkg::index::type_names::erased(name);
    let descriptor = resolve(name).or_else(|| {
        (name != definition).then(|| resolve(definition)).flatten()
    })?;
    let arguments = if descriptor.name == name && !descriptor.type_args().is_empty() {
        Box::default()
    } else {
        crate::object::type_args_from_name(name)
    };
    Some((descriptor, arguments))
}
