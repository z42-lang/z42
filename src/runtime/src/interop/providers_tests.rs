use super::*;
use crate::exec::call::builtin_abi::{BuiltinFlags, CallResult, ParamKind, RetKind};
use crate::{context::VmContext, object::Value};

fn slice(_: &VmContext, _: &[Value]) -> CallResult {
    Ok(Some(Value::I64(42)))
}
fn regs(_: &VmContext, _: Option<Value>, _: &[Value], _: &[u32]) -> CallResult {
    Ok(Some(Value::I64(42)))
}
const FIRST: BuiltinDesc = BuiltinDesc {
    name: "first",
    params: &[ParamKind::I64],
    ret: RetKind::I64,
    flags: BuiltinFlags {
        blocking: false,
        pure: true,
        reenters: false,
    },
    call_regs: regs,
    call_slice: slice,
};
const SECOND: BuiltinDesc = BuiltinDesc {
    name: "second",
    ..FIRST
};
const THIRD: BuiltinDesc = BuiltinDesc {
    name: "third",
    ..FIRST
};

#[test]
fn lookup_is_provider_scoped_and_catalog_matches_calls() {
    let mut builder = RegistryBuilder::default();
    builder
        .register(NativeProvider {
            name: "one",
            functions: &[FIRST],
        })
        .unwrap();
    builder
        .register(NativeProvider {
            name: "two",
            functions: &[SECOND],
        })
        .unwrap();
    let registry = builder.freeze();
    assert_eq!(registry.resolve("one", "first"), Some(BuiltinId(0)));
    assert_eq!(registry.resolve("two", "second"), Some(BuiltinId(1)));
    assert_eq!(registry.resolve("one", "second"), None);
    assert_eq!(registry.resolve("missing", "first"), None);
    assert!(registry.provider_functions("missing").is_none());
    assert_eq!(
        registry.provider_functions("two").unwrap()[0].name,
        "second"
    );
    let ctx = VmContext::new();
    for function in registry.functions() {
        let id = registry.catalog().id_of(function.name).unwrap();
        assert_eq!(registry.functions()[id.0 as usize].name, function.name);
        assert_eq!(
            (function.call_slice)(&ctx, &[]).unwrap(),
            Some(Value::I64(42))
        );
    }
}

#[test]
fn failed_registration_is_atomic() {
    let mut builder = RegistryBuilder::default();
    builder
        .register(NativeProvider {
            name: "one",
            functions: &[FIRST],
        })
        .unwrap();
    assert!(
        builder
            .register(NativeProvider {
                name: "two",
                functions: &[SECOND, FIRST, SECOND]
            })
            .is_err()
    );
    builder
        .register(NativeProvider {
            name: "two",
            functions: &[SECOND],
        })
        .unwrap();
    assert!(
        builder
            .register(NativeProvider {
                name: "three",
                functions: &[THIRD, THIRD]
            })
            .is_err()
    );
    builder
        .register(NativeProvider {
            name: "three",
            functions: &[THIRD],
        })
        .unwrap();
    assert_eq!(builder.freeze().functions().len(), 3);
}

#[test]
fn reject_duplicate_provider_and_empty_names() {
    let mut builder = RegistryBuilder::default();
    builder
        .register(NativeProvider {
            name: "one",
            functions: &[],
        })
        .unwrap();
    assert!(
        builder
            .register(NativeProvider {
                name: "one",
                functions: &[FIRST]
            })
            .is_err()
    );
    assert!(
        builder
            .register(NativeProvider {
                name: "",
                functions: &[FIRST]
            })
            .is_err()
    );
    assert!(
        builder
            .register(NativeProvider {
                name: "two",
                functions: &[BuiltinDesc { name: "", ..FIRST }]
            })
            .is_err()
    );
    assert!(builder.freeze().functions().is_empty());
}

#[test]
fn host_providers_have_separate_ownership() {
    let registry = crate::host::builtins::registry();
    assert!(registry.resolve("z42.core", "__str_length").is_some());
    assert!(
        registry
            .module("z42_repl")
            .unwrap()
            .function("__repl_readline")
            .is_some()
    );
    assert!(registry.resolve("z42.core", "__repl_readline").is_none());
    assert!(
        registry
            .resolve("z42.core", "__repl_member_names")
            .is_some()
    );
    #[cfg(any(feature = "native-interop", feature = "bundled-compression"))]
    assert!(
        registry
            .module("z42_compression")
            .unwrap()
            .function("__deflate_compress")
            .is_some()
    );
}

#[test]
fn same_name_in_distinct_providers_requires_a_qualified_lookup() {
    let mut builder = RegistryBuilder::default();
    builder
        .register(NativeProvider {
            name: "one",
            functions: &[FIRST],
        })
        .unwrap();
    builder
        .register(NativeProvider {
            name: "two",
            functions: &[FIRST],
        })
        .unwrap();
    let registry = builder.freeze();
    assert_eq!(registry.resolve("one", "first"), Some(BuiltinId(0)));
    assert_eq!(registry.resolve("two", "first"), Some(BuiltinId(1)));
    assert_eq!(registry.catalog().id_of("first"), None);
}
