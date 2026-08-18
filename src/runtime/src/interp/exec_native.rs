/// Native interop instructions:
///   • CallNative — Spec C2 `impl-tier1-c-abi`: libffi-driven C ABI dispatch
///   • CallNativeVtable — Spec C5 placeholder
///   • PinPtr / UnpinPtr — Spec C4/C10: zero-copy / borrowed buffer FFI
///
/// 2026-05-11 retire-z-codes:
///   - User-facing marshal failures (interior NUL on `*const c_char`,
///     PinPtr source/element type mismatch) are surfaced as
///     `Std.InvalidMarshalException` z42 exceptions, propagated up the
///     interp's `Ok(Some(Value))` channel and catchable from script.
///   - Embedder-facing failures (unknown native type / method, unimplemented
///     vtable slot) stay as `anyhow!` errors but without the legacy
///     `Z####:` prefix — the error message text is the diagnostic itself.

use crate::exception::make_stdlib_exception;
use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

use super::Frame;

const INVALID_MARSHAL_FQ: &str = "Std.InvalidMarshalException";

/// C2 (`impl-tier1-c-abi`): `CallNative` flows through the registered
/// `RegisteredType` → libffi cif → marshal/unmarshal pipeline.
/// C4/C5 will wire the remaining three opcodes.
///
/// Returns `Ok(Some(exc))` when a marshal failure produces a user-catchable
/// `Std.InvalidMarshalException`; `Ok(None)` on success; `Err` on internal
/// VM faults (unknown native type / method / arity mismatch).
pub(super) fn call_native(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    dst: u32, module_name: &str, type_name: &str, symbol: &str, args: &[u32],
) -> Result<Option<Value>> {
    use crate::native::{marshal, dispatch as ndisp};

    // Phase 2 D3+D6 wiring (2026-05-26): count + event for native FFI calls.
    // Increment fires BEFORE dispatch so even failing calls (unknown type /
    // marshal error) are counted — they still represent FFI traffic.
    ctx.counters().native_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::NativeCallEntered {
        module: module_name.to_string(),
        symbol: format!("{type_name}::{symbol}"),
    });

    let ty = ctx.resolve_native_type(module_name, type_name).ok_or_else(|| {
        anyhow::anyhow!(
            "CallNative: unknown native type {module_name}::{type_name}"
        )
    })?;
    let method = ty.method(symbol).ok_or_else(|| {
        anyhow::anyhow!(
            "CallNative: unknown method {module_name}::{type_name}::{symbol}"
        )
    })?;

    // Marshal each register into a Z42Value targeting the corresponding
    // ABI-side parameter type.
    if args.len() != method.params.len() {
        bail!(
            "CallNative {module_name}::{type_name}::{symbol}: arity mismatch (caller passed {}, signature wants {})",
            args.len(),
            method.params.len()
        );
    }
    // Spec C8: marshal arena owns temporaries (e.g. CString backing
    // for `*const c_char`) for the call's duration; dropped after
    // dispatch returns.
    let mut arena = marshal::Arena::new();
    let mut z_args: Vec<z42_abi::Z42Value> = Vec::with_capacity(method.params.len());
    for (reg, param_ty) in args.iter().zip(method.params.iter()) {
        let v = frame.get(*reg)?;
        match marshal::value_to_z42(v, param_ty, &mut arena) {
            Ok(z)                                       => z_args.push(z),
            Err(marshal::MarshalErr::InvalidMarshal(m)) => {
                let exc = make_stdlib_exception(ctx, module, INVALID_MARSHAL_FQ, m)?;
                return Ok(Some(exc));
            }
            Err(marshal::MarshalErr::Internal(e))       => return Err(e),
        }
    }

    // SAFETY: cif was built from `params`/`return_type` matching the
    // native function pointer at registration time; native lib keeps
    // the function alive via `VmContext.native_libs`. CURRENT_VM is
    // set by VmGuard so a re-entrant z42_* call finds the right ctx.
    let z_ret = unsafe {
        ndisp::call(
            &method.cif,
            method.fn_ptr,
            &z_args,
            &method.params,
            &method.return_type,
        )
    }?;
    drop(arena);

    let result = marshal::z42_to_value(&z_ret, &method.return_type)?;
    frame.set(dst, result);
    Ok(None)
}

pub(super) fn call_native_vtable(vtable_slot: u16) -> Result<()> {
    bail!(
        "CallNativeVtable not yet implemented (spec C5 / impl-source-generator): slot={vtable_slot}"
    );
}

/// C4: borrow a `String` / future `Array<u8>` buffer for FFI.
/// Caller (currently always test-emitted IR; user-side `pinned`
/// syntax lands in C5) is responsible for matching `UnpinPtr` on
/// every exit path. RC backend treats the borrow as zero-cost
/// (no relocation possible); the pin set will be repopulated
/// for moving GC backends in a later spec.
///
/// 2026-05-11 retire-z-codes: type-mismatch + element-out-of-range
/// surface as `Std.InvalidMarshalException` (user-catchable). Returns
/// `Ok(Some(exc))` to propagate via interp's value-based throw channel.
pub(super) fn pin_ptr(
    ctx: &VmContext, module: &Module, frame: &mut Frame, dst: u32, src: u32,
) -> Result<Option<Value>> {
    let fid = frame.frame_id;
    // make-value-copy: PinnedView payload lives in the per-context transient arena;
    // the register holds only an 8B `{idx, frame_id}` handle.
    let mk_view = |ctx: &VmContext, data: crate::metadata::PinnedViewData| {
        let idx = ctx.transient_arena.lock()
            .alloc(fid, crate::interp::transient_arena::TransientPayload::PinView(data));
        Value::PinnedView { idx, frame_id: fid }
    };
    let view = match frame.get(src)? {
        Value::Str(s) => mk_view(ctx, crate::metadata::PinnedViewData {
            ptr: s.as_ptr() as u64,
            len: s.len() as u64,
            kind: crate::metadata::PinSourceKind::Str,
        }),
        Value::Array(arr) => {
            // Spec C10 — `Array<u8>` pin: snapshot the bytes into
            // a Box<[u8]> owned by the VM for the pin's lifetime.
            // Each element must be a `Value::I64` in 0..=255.
            let arr_ref = arr.borrow();
            // packed-primitive-arrays: a packed `byte[]` (`Bytes` backing) is a
            // contiguous `&[u8]` — copy it straight out, no per-element unbox
            // (the "简化 extern call" win). Boxed falls back to validated unbox.
            let bytes: Vec<u8> = if let Some(b) = arr_ref.as_bytes() {
                b.to_vec()
            } else {
                let n_elems = arr_ref.len();
                let mut out = Vec::with_capacity(n_elems);
                let mut invalid: Option<(usize, String)> = None;
                for i in 0..n_elems {
                    match arr_ref.get_boxed(i) {
                        Value::I64(n) if (0..=255).contains(&n) => out.push(n as u8),
                        other => { invalid = Some((i, format!("{other:?}"))); break; }
                    }
                }
                if let Some((i, o)) = invalid {
                    drop(arr_ref);
                    let msg = format!("PinPtr Array element {i} not a u8 in 0..=255: {o}");
                    let exc = make_stdlib_exception(ctx, module, INVALID_MARSHAL_FQ, msg)?;
                    return Ok(Some(exc));
                }
                out
            };
            let len = bytes.len() as u64;
            let buf: Box<[u8]> = bytes.into_boxed_slice();
            let ptr = ctx.pin_owned_buffer(buf);
            mk_view(ctx, crate::metadata::PinnedViewData {
                ptr,
                len,
                kind: crate::metadata::PinSourceKind::ArrayU8,
            })
        }
        other => {
            let msg = format!(
                "PinPtr source must be String or Array<u8>, got {:?}", other
            );
            let exc = make_stdlib_exception(ctx, module, INVALID_MARSHAL_FQ, msg)?;
            return Ok(Some(exc));
        }
    };
    frame.set(dst, view);
    Ok(None)
}

pub(super) fn unpin_ptr(ctx: &VmContext, frame: &Frame, pinned: u32) -> Result<()> {
    match frame.get(pinned)? {
        // make-value-copy: resolve the PinnedView handle → (kind, ptr) via the arena.
        &Value::PinnedView { idx, frame_id } => {
            let (kind, ptr) = ctx.transient_arena.lock().with(idx, frame_id, |p| match p {
                crate::interp::transient_arena::TransientPayload::PinView(pv) => (pv.kind, pv.ptr),
                _ => (crate::metadata::PinSourceKind::Str, 0),
            })?;
            if kind == crate::metadata::PinSourceKind::ArrayU8 {
                // Spec C10: drop the snapshot Box<[u8]> we leaked into VmContext at PinPtr time.
                ctx.release_owned_buffer(ptr);
            }
            // Str pin: borrowed from the source String — no-op (future moving GC would
            // deregister the pin-set entry here).
            Ok(())
        }
        other => bail!(
            "UnpinPtr expects PinnedView (compiler-emitted UnpinPtr should always pair with a prior PinPtr); got {:?}",
            other
        ),
    }
}
