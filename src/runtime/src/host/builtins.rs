//! Default native providers selected by the host, independent of corelib implementation.
use crate::{
    exec::call::builtin_abi::BuiltinDesc,
    interop::providers::{NativeProvider, NativeRegistry, RegistryBuilder},
};
use std::sync::OnceLock;

pub(crate) fn registry() -> &'static NativeRegistry {
    static REGISTRY: OnceLock<NativeRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut builder = RegistryBuilder::default();
        let providers = [
            NativeProvider {
                name: "z42.core",
                functions: crate::corelib::builtins::DESCS,
            },
            #[cfg(test)]
            NativeProvider {
                name: "z42.test",
                functions: crate::exec::call::builtin_abi::test_builtins::DESCS,
            },
        ];
        for provider in providers {
            if let Err(error) = builder.register(provider) {
                panic!("invalid default native provider assembly: {error:#}");
            }
        }
        if let Err(error) = unsafe { builder.register_module(z42_repl::z42_repl_module_v1()) } {
            panic!("invalid REPL module registration: {error:#}");
        }
        #[cfg(feature = "bundled-compression")]
        let compression =
            unsafe { builder.register_module(z42_compression::z42_compression_module_v1()) };
        #[cfg(not(feature = "bundled-compression"))]
        let compression =
            unsafe { builder.register_declaration(z42_compression::registration::declaration()) };
        if let Err(error) = compression {
            panic!("invalid compression module registration: {error:#}");
        }
        builder.freeze()
    })
}

pub(crate) fn functions() -> &'static [BuiltinDesc] {
    registry().functions()
}
pub(crate) fn catalog() -> &'static crate::metadata::builtins::BuiltinCatalog {
    registry().catalog()
}
