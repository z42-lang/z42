#![allow(dangerous_implicit_autorefs)]
//! Array allocation, element access, length.

use crate::metadata::types::default_value_for_tag;
use crate::metadata::Value;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_new(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, size: u32, elem_tag: u8,
    // add-reflection-array-element-type: element type FQ name (ptr,len) from the
    // instruction's module-lifetime String — non-erased array reflection.
    et_ptr: *const u8, et_len: usize,
) -> u8 {
    let n = match &(*frame).regs[size as usize] {
        Value::I64(n) if *n >= 0 => *n as usize,
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArrayNew: expected non-negative int, got {:?}", other).into()));
            return 1;
        }
    };
    let element_type = std::str::from_utf8(std::slice::from_raw_parts(et_ptr, et_len)).unwrap_or("");
    // add-struct-jit-value-path (P5): value-struct element → StructBytes heap backing
    // (mirrors interp `array_new`); otherwise `arr[i]` can't materialize a
    // StructRefHeap and the struct field access on it would see a Null base.
    if let Some(sb) = crate::interp::exec_array::try_struct_backed(vm_ctx_ref(ctx), element_type, n) {
        let arr = vm_ctx_ref(ctx).heap().alloc_array_obj(sb);
        if matches!(arr, Value::Null) {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("cannot allocate struct array[{n}]: heap limit exceeded").into()));
            return 1;
        }
        (*frame).regs[dst as usize] = arr;
        return 0;
    }
    let default = default_value_for_tag(elem_tag);
    (*frame).regs[dst as usize] = vm_ctx_ref(ctx).heap().alloc_array_typed(element_type, vec![default; n]);
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_new_lit(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, elems_ptr: *const u32, elem_cnt: usize,
    et_ptr: *const u8, et_len: usize,
) -> u8 {
    let elems = std::slice::from_raw_parts(elems_ptr, elem_cnt);
    let vals: Vec<Value> = elems.iter().map(|&r| (*frame).regs[r as usize].clone()).collect();
    let element_type = std::str::from_utf8(std::slice::from_raw_parts(et_ptr, et_len)).unwrap_or("");
    // add-struct-jit-value-path (P5): value-struct literal → StructBytes backing,
    // packing each element's bytes + reference leaves (mirrors interp array_new_lit).
    if let Some(mut sb) = crate::interp::exec_array::try_struct_backed(vm_ctx_ref(ctx), element_type, vals.len()) {
        for (i, v) in vals.iter().enumerate() {
            if let Err(e) = crate::interp::exec_array::pack_struct_elem(vm_ctx_ref(ctx), &mut sb, i, v) {
                set_exception(vm_ctx_ref(ctx), Value::Str(format!("{e}").into()));
                return 1;
            }
        }
        let arr = vm_ctx_ref(ctx).heap().alloc_array_obj(sb);
        if matches!(arr, Value::Null) {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("cannot allocate struct array literal[{}]: heap limit exceeded", vals.len()).into()));
            return 1;
        }
        (*frame).regs[dst as usize] = arr;
        return 0;
    }
    (*frame).regs[dst as usize] = vm_ctx_ref(ctx).heap().alloc_array_typed(element_type, vals);
    0
}

/// Phase 4a (jit-inline-fastpaths): expose the array's element data pointer +
/// length so the JIT can do a **native** bounds-check + element load, instead of
/// the full `jit_array_get` round-trip through a boxed `Value`. Safe: uses real
/// types; the returned `*const Value` points into the array's `Vec` heap buffer,
/// which stays put for the duration of the calling instruction (single-threaded
/// read; the array isn't reallocated mid-read). Returns 0 + writes
/// `*out_ptr`/`*out_len` on success; 1 (exception set) if the reg isn't an array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_data(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    arr: u32, out_ptr: *mut *const Value, out_len: *mut i64, out_width: *mut i64,
) -> u8 {
    match &(*frame).regs[arr as usize] {
        Value::Array(rc) => {
            let borrowed = rc.borrow();
            // jit-inline-i32-arrays: hand back the packed buffer base (`int[]`/
            // `long[]`/`double[]`) plus the runtime slot width (4/8; 0 if the
            // backing isn't packed-numeric). The ArrayGet inline uses the
            // compile-time element width; the ArraySet inline consults
            // `out_width` so a narrowing store writes the right slot size.
            *out_ptr = borrowed.packed_num_ptr().unwrap_or(std::ptr::null()) as *const Value;
            *out_len = borrowed.len() as i64;
            *out_width = borrowed.packed_elem_width();
            0
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(
                format!("ArrayGet: expected array, got {:?}", other).into()));
            1
        }
    }
}

/// Phase 4b (jit-inline-fastpaths 方案 B): **non-throwing** array-data fetch for
/// the loop-invariant hoist. Emitted ONCE in the JIT entry block for array
/// registers proven never-reassigned. On success writes ptr+len; if the reg
/// isn't an array (incl. null) it writes `*out_ptr = null` and **does not throw**
/// — the per-`ArrayGet` inline detects the null ptr and falls back to
/// `jit_array_get`, so the exception fires at the real access point (no
/// spurious throw when the array is never actually indexed / loop runs 0 times).
/// GC-safe: z42 arrays are fixed-length (no realloc) and the collector is
/// non-moving, so the returned buffer ptr stays valid for the function's life.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_data_opt(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    arr: u32, out_ptr: *mut *const Value, out_len: *mut i64, out_width: *mut i64,
) {
    match &(*frame).regs[arr as usize] {
        Value::Array(rc) => {
            let borrowed = rc.borrow();
            // jit-inline-i32-arrays: packed buffer base + runtime slot width.
            *out_ptr = borrowed.packed_num_ptr().unwrap_or(std::ptr::null()) as *const Value;
            *out_len = borrowed.len() as i64;
            *out_width = borrowed.packed_elem_width();
        }
        _ => {
            *out_ptr = std::ptr::null();
            *out_len = 0;
            *out_width = 0;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_get(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, arr: u32, idx: u32,
) -> u8 {
    let arr_val = (*frame).regs[arr as usize].clone();
    let idx_val = (*frame).regs[idx as usize].clone();
    let result = match &arr_val {
        // fix-jit-osr-stackarray: under OSR the interp portion may have created a
        // stack-allocated array (escape analysis) that is live in `frame.regs`
        // when the JIT takes over. Mirror interp `exec_array::array_get` — resolve
        // via the per-context stack arena. (Non-OSR JIT never produces a
        // StackArray, so this arm only fires on the OSR entry path.)
        Value::StackArray { idx: aidx, frame_id } => {
            let (aidx, frame_id) = (*aidx, *frame_id);
            let i = match &idx_val {
                Value::I64(n) if *n >= 0 => *n as usize,
                other => {
                    set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArrayGet: bad index {:?}", other).into()));
                    return 1;
                }
            };
            let res = vm_ctx_ref(ctx).stack_arena.lock().with_arr(aidx, frame_id, |a| {
                if i >= a.len() {
                    return Err(format!("array index {} out of bounds (len={})", i, a.len()));
                }
                Ok(a.get_boxed(i))
            });
            match res {
                Ok(Ok(v)) => v,
                Ok(Err(msg)) => { set_exception(vm_ctx_ref(ctx), Value::Str(msg.into())); return 1; }
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
            }
        }
        Value::Array(rc) => {
            let i = match &idx_val {
                Value::I64(n) if *n >= 0 => *n as usize,
                Value::I64(n) if *n >= 0 => *n as usize,
                other => {
                    set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArrayGet: bad index {:?}", other).into()));
                    return 1;
                }
            };
            let borrowed = rc.borrow();
            if i >= borrowed.len() {
                set_exception(vm_ctx_ref(ctx), Value::Str(format!("array index {} out of bounds (len={})", i, borrowed.len()).into()));
                return 1;
            }
            // add-struct-jit-value-path (P5): a value-struct array element is a
            // `StructRefHeap` handle into the array's byte backing (for in-place
            // `arr[i].x` / value-copy at consumers), mirroring interp `array_get`
            // (add-struct-array-codegen). Without this the element would degrade to
            // a `get_boxed` BoxedStruct snapshot and the following StructFieldGetPrim
            // (base = StructRefHeap/StructRef) would mismatch.
            if matches!(&borrowed.backing, crate::metadata::types::ArrayBacking::StructBytes { .. }) {
                let arr_gc = rc.clone();
                drop(borrowed);
                Value::StructRefHeap(Box::new(crate::metadata::types::StructArrayElem {
                    arr: arr_gc,
                    index: i as u32,
                }))
            } else {
                borrowed.get_boxed(i)
            }
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArrayGet: expected array, got {:?}", other).into()));
            return 1;
        }
    };
    (*frame).regs[dst as usize] = result;
    0
}

/// JIT ArraySet helper.
///
/// **add-write-barriers (2026-05-21)**: dispatches `write_barrier_array_elem`
/// after a successful element write *iff* `v.is_heap_ref()`.
/// Mirrors `interp::exec_array::array_set`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_set(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    arr: u32, idx: u32, val: u32,
) -> u8 {
    let arr_val = (*frame).regs[arr as usize].clone();
    let idx_val = (*frame).regs[idx as usize].clone();
    let v       = (*frame).regs[val as usize].clone();
    match &arr_val {
        // fix-jit-osr-stackarray: OSR-entry stack array — write via the arena,
        // mirroring interp `exec_array::array_set`. No GC write barrier (not a
        // heap slot; stack-array heap-ref elems are kept live by the arena root
        // scan). See `jit_array_get`.
        Value::StackArray { idx: aidx, frame_id } => {
            let (aidx, frame_id) = (*aidx, *frame_id);
            let i = match &idx_val {
                Value::I64(n) if *n >= 0 => *n as usize,
                other => {
                    set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArraySet: bad index {:?}", other).into()));
                    return 1;
                }
            };
            let res = vm_ctx_ref(ctx).stack_arena.lock().with_arr_mut(aidx, frame_id, |a| {
                if i >= a.len() {
                    return Err(format!("array index {} out of bounds (len={})", i, a.len()));
                }
                a.set_boxed(i, v.clone());
                Ok(())
            });
            match res {
                Ok(Ok(())) => {}
                Ok(Err(msg)) => { set_exception(vm_ctx_ref(ctx), Value::Str(msg.into())); return 1; }
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
            }
        }
        Value::Array(rc) => {
            let i = match &idx_val {
                Value::I64(n) if *n >= 0 => *n as usize,
                Value::I64(n) if *n >= 0 => *n as usize,
                other => {
                    set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArraySet: bad index {:?}", other).into()));
                    return 1;
                }
            };
            let mut borrowed = rc.borrow_mut();
            if i >= borrowed.len() {
                set_exception(vm_ctx_ref(ctx), Value::Str(format!("array index {} out of bounds (len={})", i, borrowed.len()).into()));
                return 1;
            }
            borrowed.set_boxed(i, v.clone());
            drop(borrowed);
            if v.is_heap_ref() {
                vm_ctx_ref(ctx).heap().write_barrier_array_elem(&arr_val, i, &v);
            }
        }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArraySet: expected array, got {:?}", other).into()));
            return 1;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_array_len(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, arr: u32,
) -> u8 {
    // fix-jit-osr-stackarray: clone the handle so we can drop the `frame.regs`
    // borrow before locking the arena (arena ops may run GC root scans).
    let arr_val = (*frame).regs[arr as usize].clone();
    match &arr_val {
        // OSR-entry stack array — length via arena, mirroring interp array_len.
        Value::StackArray { idx: aidx, frame_id } => {
            let (aidx, frame_id) = (*aidx, *frame_id);
            match vm_ctx_ref(ctx).stack_arena.lock().with_arr(aidx, frame_id, |a| a.len() as i64) {
                Ok(len) => { (*frame).regs[dst as usize] = Value::I64(len); 0 }
                Err(e) => {
                    set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into()));
                    1
                }
            }
        }
        Value::Array(rc) => { (*frame).regs[dst as usize] = Value::I64(rc.borrow().len() as i64); 0 }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("ArrayLen: expected array, got {:?}", other).into()));
            1
        }
    }
}
