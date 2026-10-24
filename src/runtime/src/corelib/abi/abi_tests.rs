use super::*;

/// Error kind `k` for `__test_abi_fail`.
pub(crate) fn kind(k: i64) -> BuiltinError {
    let m = || format!("kind {k}");
    match k {
        0 => BuiltinError::Format(m()),
        1 => BuiltinError::Argument(m()),
        2 => BuiltinError::ArgumentNull(m()),
        3 => BuiltinError::ArgumentOutOfRange(m()),
        4 => BuiltinError::IndexOutOfRange(m()),
        5 => BuiltinError::Overflow(m()),
        6 => BuiltinError::InvalidOperation(m()),
        7 => BuiltinError::NotSupported(m()),
        8 => BuiltinError::InvalidCast(m()),
        9 => BuiltinError::NullReference(m()),
        10 => BuiltinError::KeyNotFound(m()),
        11 => BuiltinError::Io(m()),
        12 => BuiltinError::FileNotFound(m()),
        13 => BuiltinError::DirectoryNotFound(m()),
        14 => BuiltinError::PathTooLong(m()),
        15 => BuiltinError::EndOfStream(m()),
        16 => BuiltinError::UnauthorizedAccess(m()),
        17 => BuiltinError::Timeout(m()),
        _ => BuiltinError::Exception { class: "Std.Threading.ThreadException", message: m() },
    }
}

use crate::corelib::{builtin_id_of, invoke_builtin, BUILTINS};
use crate::metadata::tokens::TypeId;
use crate::metadata::types::{FieldSlot, TypeDesc};
use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use std::sync::Arc;

fn s(t: &str) -> Value { Value::Str(t.into()) }

fn id(name: &str) -> u32 { builtin_id_of(name).unwrap_or_else(|| panic!("{name} registered")).0 }

/// Call `name` the way the engines do: arguments read from `regs` by index.
fn call_regs(ctx: &VmContext, name: &str, this: Option<Value>, regs: &[Value], idx: &[u32])
    -> Result<Option<Value>, BuiltinError>
{
    invoke_builtin(ctx, id(name), name, this, regs, idx).map_err(|e| *e)
}

#[test]
fn typed_builtins_are_in_the_table_once() {
    let mut seen = std::collections::HashSet::new();
    for d in BUILTINS { assert!(seen.insert(d.name), "builtin `{}` registered twice", d.name); }
    let d = &BUILTINS[id("__test_abi_args") as usize];
    assert_eq!(d.params, &[ParamKind::Str, ParamKind::I64, ParamKind::F64, ParamKind::Bool,
        ParamKind::Char, ParamKind::OptStr]);
    assert_eq!(d.ret, RetKind::Str);
    assert!(!d.flags.blocking);
    assert!(BUILTINS[id("__test_abi_sleep") as usize].flags.blocking);
    let d = &BUILTINS[id("__int32_parse") as usize];
    assert!(d.flags.pure && d.ret == RetKind::I64);
    // An array parameter is taken by handle (`ArrRef`): `ParamKind::Array`.
    assert_eq!(BUILTINS[id("__array_clone") as usize].params, &[ParamKind::Array]);
}

/// `ArrRef` parameters: null → `ArgumentNullException` (`NullReferenceException` as the
/// receiver), any other kind → `InvalidCastException`; an array comes through by handle.
#[test]
fn array_arguments_are_taken_by_handle() {
    let ctx = VmContext::new();
    let arr = ctx.heap().alloc_array(vec![Value::I64(1), Value::I64(2)]);
    let regs = [arr, Value::I64(0), Value::Null, s("x")];
    // `__array_copy(src, 0, dst, 0, 2)` with src = dst: a legal (no-op) overlapping copy.
    assert_eq!(call_regs(&ctx, "__array_copy", None, &regs, &[0, 1, 0, 1, 1]).unwrap(), None);
    match call_regs(&ctx, "__array_copy", None, &regs, &[2, 1, 0, 1, 1]) {
        Err(BuiltinError::ArgumentNull(m)) => assert_eq!(m, "Value cannot be null. (Parameter 'src')"),
        other => panic!("{other:?}"),
    }
    match call_regs(&ctx, "__array_copy", None, &regs, &[3, 1, 0, 1, 1]) {
        Err(BuiltinError::InvalidCast(m)) => assert!(m.contains("expects an array"), "{m}"),
        other => panic!("{other:?}"),
    }
    // As a receiver (`this = "Clone"`), null is a null-reference, not a null argument.
    match call_regs(&ctx, "__array_clone", Some(Value::Null), &[], &[]) {
        Err(BuiltinError::NullReference(m)) => assert!(m.contains("Clone"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn arguments_are_read_in_place_from_the_register_file() {
    let ctx = VmContext::new();
    // Registers deliberately out of order: the wrapper indexes them, nothing is collected.
    let regs = [Value::Null, Value::Char('x'), Value::Bool(true), Value::F64(2.5), Value::I64(3), s("a")];
    let r = call_regs(&ctx, "__test_abi_args", None, &regs, &[5, 4, 3, 2, 1, 0]).unwrap();
    assert_eq!(r, Some(s("a|3|2.5|true|x|<null>")));
    // An `object`-typed call site boxes its integer: a boxed int reads as `i64` too
    // (exercised through the slice entry, as `exec_builtin*` callers use it).
    let r = crate::corelib::exec_builtin_value(&ctx, "__test_abi_args",
        &[s("b"), Value::I64(-1), Value::I64(4), Value::Bool(false), Value::Char('y'), s("o")]).unwrap();
    assert_eq!(r, s("b|-1|4|false|y|o"));
}

#[test]
fn a_receiver_outside_the_registers_comes_first() {
    let ctx = VmContext::new();
    assert_eq!(call_regs(&ctx, "__test_abi_recv", Some(s("abcd")), &[], &[]).unwrap(), Some(Value::I64(4)));
    assert_eq!(call_regs(&ctx, "__test_abi_add", Some(Value::I64(40)), &[Value::I64(2)], &[0]).unwrap(),
        Some(Value::I64(42)));
}

#[test]
fn argument_errors_are_typed() {
    let ctx = VmContext::new();
    let regs = [s("a"), Value::I64(3), Value::F64(2.5), Value::Bool(true), Value::Char('x'), Value::Null, Value::Null];
    // null where a value is required → ArgumentNullException, named by the Rust parameter.
    match call_regs(&ctx, "__test_abi_args", None, &regs, &[6, 1, 2, 3, 4, 5]) {
        Err(BuiltinError::ArgumentNull(m)) => assert_eq!(m, "Value cannot be null. (Parameter 's')"),
        other => panic!("{other:?}"),
    }
    // a null receiver → NullReferenceException with the `VCall` text.
    match call_regs(&ctx, "__test_abi_recv", Some(Value::Null), &[], &[]) {
        Err(BuiltinError::NullReference(m)) => assert_eq!(m, "cannot call method `Recv` on a null reference"),
        other => panic!("{other:?}"),
    }
    // another kind than the parameter takes → InvalidCastException.
    match call_regs(&ctx, "__test_abi_args", None, &regs, &[1, 1, 2, 3, 4, 5]) {
        Err(BuiltinError::InvalidCast(m)) => assert!(m.contains("`s` expects a string"), "{m}"),
        other => panic!("{other:?}"),
    }
    // the wrong argument count is a declaration bug → internal.
    match call_regs(&ctx, "__test_abi_add", None, &regs, &[1]) {
        Err(BuiltinError::Internal(m)) => assert!(m.contains("takes 2 argument(s), the call passes 1"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ref_arguments_are_stored_through() {
    use crate::interp::transient_arena::TransientPayload;
    use crate::metadata::types::RefKind;
    let ctx = VmContext::new();
    // A `ref` to slot 0 of a frame on the stack (what `LoadLocalAddr` makes).
    let target: Vec<Value> = vec![Value::I64(0)];
    ctx.push_frame(crate::exception::VmFrame::new(std::ptr::null(), &target));
    let frame_id = ctx.next_frame_id();
    let h = ctx.transient_alloc(frame_id, TransientPayload::Ref(RefKind::Stack { frame_idx: 0, slot: 0 }));
    let r = Value::Ref { idx: h, frame_id };
    assert_eq!(call_regs(&ctx, "__test_abi_store", None, &[r, Value::I64(7)], &[0, 1]).unwrap(), None);
    assert_eq!(target[0], Value::I64(7));
    ctx.pop_frame();
    // A plain value where the location would be (reflection passes values): the store
    // goes nowhere, no error.
    assert_eq!(call_regs(&ctx, "__test_abi_store", None, &[Value::I64(0), Value::I64(7)], &[0, 1]).unwrap(), None);
}

// ── Error kinds → exception classes ──────────────────────────────────────────

fn exception_type(name: &str, base: Option<&str>) -> Arc<TypeDesc> {
    let fields = vec![
        FieldSlot { name: "Message".into(),        type_tag: "str".into(), visibility: 0 },
        FieldSlot { name: "StackTrace".into(),     type_tag: "str".into(), visibility: 0 },
        FieldSlot { name: "InnerException".into(), type_tag: "Std.Exception".into(), visibility: 0 },
    ];
    let mut field_index = crate::metadata::NameIndex::new();
    for (i, f) in fields.iter().enumerate() { field_index.insert(f.name.to_string(), i); }
    let own: Box<[FieldSlot]> = fields.clone().into();
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.into(), id: TypeId::UNRESOLVED,
        base_name: base.map(str::to_owned), fields, field_index,
        vtable: vec![], vtable_index: crate::metadata::NameIndex::new(),
        cold: Some(Box::new(crate::metadata::types::TypeDescCold { own_fields: own, ..Default::default() })),
    })
}

fn module_with(classes: &[&str]) -> Module {
    let mut type_registry = rustc_hash::FxHashMap::default();
    type_registry.insert("Std.Exception".to_string(), exception_type("Std.Exception", None));
    for c in classes {
        type_registry.insert(c.to_string(), exception_type(c, Some("Std.Exception")));
    }
    Module {
        name: "M".into(), string_pool: vec![], classes: vec![], functions: vec![],
        type_registry, func_index: rustc_hash::FxHashMap::default(),
    }
}

fn thrown_class(r: error::Raised) -> String {
    match r {
        error::Raised::Throw(Value::Object(o)) => o.type_desc().name.to_string(),
        error::Raised::Throw(v) => panic!("not an object: {v:?}"),
        error::Raised::Fatal(e) => panic!("fatal: {e}"),
        error::Raised::Raw(e) => format!("raw: {e}"),
    }
}

/// Every kind throws its .NET class when it is loaded …
#[test]
fn each_error_kind_throws_its_class() {
    let expected = [
        "Std.FormatException", "Std.ArgumentException", "Std.ArgumentNullException",
        "Std.ArgumentOutOfRangeException", "Std.IndexOutOfRangeException", "Std.OverflowException",
        "Std.InvalidOperationException", "Std.NotSupportedException", "Std.InvalidCastException",
        "Std.NullReferenceException", "Std.KeyNotFoundException", "Std.IO.IOException",
        "Std.IO.FileNotFoundException", "Std.IO.DirectoryNotFoundException", "Std.IO.PathTooLongException",
        "Std.IO.EndOfStreamException", "Std.UnauthorizedAccessException", "Std.TimeoutException",
        "Std.Threading.ThreadException",
    ];
    let m = module_with(&expected);
    let ctx = VmContext::new();
    for (k, class) in expected.iter().enumerate() {
        let e = kind(k as i64);
        let msg = e.message().into_owned();
        let r = error::raise(&ctx, &m, "__test_abi_fail", e, &[]);
        let v = match &r { error::Raised::Throw(v) => *v, _ => panic!("kind {k}") };
        assert_eq!(thrown_class(r), *class, "kind {k}");
        assert_eq!(crate::exception::read_message(&v, &ctx, &m).as_deref(), Some(msg.as_str()));
    }
}

/// … the nearest loaded base otherwise (`Std.Exception` last), and only with no
/// exception class at all is the error left raw.
#[test]
fn an_unloaded_class_falls_back_to_its_base() {
    let ctx = VmContext::new();
    let m = module_with(&["Std.IO.IOException", "Std.ArithmeticException"]);
    let class = |e| thrown_class(error::raise(&ctx, &m, "x", e, &[]));
    assert_eq!(class(BuiltinError::FileNotFound("f".into())), "Std.IO.IOException");
    assert_eq!(class(BuiltinError::Overflow("o".into())), "Std.ArithmeticException");
    assert_eq!(class(BuiltinError::Format("f".into())), "Std.Exception");
    let bare = Module { type_registry: Default::default(), ..module_with(&[]) };
    assert_eq!(thrown_class(error::raise(&ctx, &bare, "x", BuiltinError::Format("f".into()), &[])),
        "raw: Std.FormatException: f");
}

#[test]
fn a_typed_error_survives_the_anyhow_channel() {
    // Slice callers (`exec_builtin*`) see `anyhow`; `raise` still finds the kind.
    let ctx = VmContext::new();
    let e = crate::corelib::exec_builtin(&ctx, "__int32_parse", &[s("x")]).unwrap_err();
    let m = module_with(&["Std.FormatException"]);
    assert_eq!(thrown_class(error::raise(&ctx, &m, "x", BuiltinError::Legacy(e), &[])), "Std.FormatException");
    // An old-table error keeps the old mapping.
    assert_eq!(thrown_class(error::raise(&ctx, &m, "x", BuiltinError::Legacy(anyhow::anyhow!("boom")), &[])),
        "Std.Exception");
}

#[test]
fn an_unknown_builtin_is_a_missing_symbol() {
    let ctx = VmContext::new();
    let r = invoke_builtin(&ctx, crate::metadata::tokens::UNRESOLVED, "__no_such_builtin", None, &[], &[]);
    let m = module_with(&["Std.MissingSymbolException"]);
    assert_eq!(thrown_class(error::raise(&ctx, &m, "__no_such_builtin", *r.unwrap_err(), &[])),
        "Std.MissingSymbolException");
}

#[test]
fn io_errors_map_like_dotnet() {
    use std::io::{Error, ErrorKind};
    assert!(matches!(BuiltinError::from(Error::from(ErrorKind::NotFound)), BuiltinError::FileNotFound(_)));
    assert!(matches!(BuiltinError::from(Error::from(ErrorKind::PermissionDenied)), BuiltinError::UnauthorizedAccess(_)));
    assert!(matches!(BuiltinError::from(Error::from(ErrorKind::TimedOut)), BuiltinError::Timeout(_)));
    assert!(matches!(BuiltinError::from(Error::from(ErrorKind::UnexpectedEof)), BuiltinError::EndOfStream(_)));
    assert!(matches!(BuiltinError::from(Error::from(ErrorKind::Other)), BuiltinError::Io(_)));
}

// ── Blocking ─────────────────────────────────────────────────────────────────

fn parked(ctx: &VmContext) -> u64 {
    ctx.core.parked_count.load(std::sync::atomic::Ordering::Acquire) as u64
}

fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let t0 = std::time::Instant::now();
    while t0.elapsed() < std::time::Duration::from_secs(5) {
        if cond() { return true; }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    cond()
}

/// A `blocking` builtin sleeps parked — a collection another thread starts meanwhile
/// does not wait for it — and its result is allocated only after it unparks (a
/// `String` built while parked would trip `debug_assert_not_native_parked`).
#[test]
fn a_blocking_builtin_lets_a_collection_run_while_it_waits() {
    for name in ["__test_abi_sleep", "__test_abi_prepared"] {
        let ctx = VmContext::new();
        let core = Arc::clone(&ctx.core);
        let sleeper = std::thread::spawn(move || {
            let w = VmContext::new_with_core(core);
            crate::corelib::exec_builtin_value(&w, name, &[Value::I64(1500)]).unwrap()
        });
        assert!(wait_until(|| parked(&ctx) == 1), "{name}: the sleeping thread is parked");
        let t0 = std::time::Instant::now();
        {
            let pause = crate::gc::safepoint::request_gc_pause(&ctx);
            assert!(pause.is_some(), "{name}: this thread becomes the collector");
            ctx.heap().collect_full();
        }
        assert!(t0.elapsed() < std::time::Duration::from_millis(1000),
            "{name}: the collection did not wait for the sleeper ({:?})", t0.elapsed());
        let r = sleeper.join().unwrap();
        assert!(matches!(&r, Value::Str(t) if t.starts_with("slept") || t.starts_with("depth")), "{r:?}");
        assert_eq!(parked(&ctx), 0, "{name}: the park is released");
    }
}
