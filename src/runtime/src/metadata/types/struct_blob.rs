//! Value structs resident on the heap (object model R3): an inline struct field of an object, a
//! boxed struct, a `struct[]` element.
//!
//! A heap blob has exactly the compiler's struct layout (`StructTypeLayout`): primitive leaves at
//! their natural width, and every reference leaf an 8 B self-describing reference word
//! (`ref_word`) in the 8 B slot the compiler reserves for it. An object's inline struct is
//! therefore byte-identical to the standalone blob of a boxed struct or a `struct[]` element —
//! one layout, one set of offsets.
//!
//! The frame arena (`interp::struct_arena`) keeps the register-side form instead: `bytes` plus
//! the reference leaves as 16 B `Value`s, in `ref_offsets` order (it is a GC root, like the
//! registers). [`read`] / [`write`] convert between the two; every reference store goes through
//! [`ref_word::store`] (SATB on the replaced word, a box when the value does not fit 8 B), and the
//! caller hands each stored reference to its write barrier.

use super::{ref_word, Cells, CellsMut, Value};

/// Copy the blob of `size` bytes at `base` out into the arena form: the bytes (with every
/// reference slot zeroed — the copy never carries handle bits) and the reference leaves decoded
/// in `ref_offsets` order (offsets are blob-relative).
pub fn read(c: &Cells, base: usize, size: usize, ref_offsets: &[u32]) -> (Vec<u8>, Vec<Value>) {
    let mut bytes = vec![0u8; size];
    c.copy_out(base, &mut bytes);
    let mut refs = Vec::with_capacity(ref_offsets.len());
    for &ro in ref_offsets {
        let ro = ro as usize;
        if let Some(slot) = bytes.get_mut(ro..ro + 8) {
            slot.fill(0);
        }
        refs.push(ref_word::load(c, base + ro));
    }
    (bytes, refs)
}

/// Copy the arena form `(src_bytes, src_refs)` into the blob of `size` bytes at `base`.
/// Primitive bytes are copied around the reference slots; each reference leaf is stored as a
/// reference word and the reference that landed (the box, for a value that did not fit) goes to
/// `barrier`. A missing `src_refs` entry stores `null`.
pub fn write(
    c: &CellsMut, base: usize, size: usize, src_bytes: &[u8], src_refs: &[Value], ref_offsets: &[u32],
    barrier: &mut dyn FnMut(&Value),
) -> anyhow::Result<()> {
    let n = size.min(src_bytes.len());
    let gaps = |from: usize, to: usize| {
        if from < to.min(n) {
            c.copy_in(base + from, &src_bytes[from..to.min(n)]);
        }
    };
    if ref_offsets.windows(2).all(|w| w[0] < w[1]) {
        let mut cur = 0usize;
        for &ro in ref_offsets {
            gaps(cur, ro as usize);
            cur = ro as usize + 8;
        }
        gaps(cur, n);
    } else {
        let mut sorted = ref_offsets.to_vec();
        sorted.sort_unstable();
        let mut cur = 0usize;
        for ro in sorted {
            gaps(cur, ro as usize);
            cur = ro as usize + 8;
        }
        gaps(cur, n);
    }
    for (k, &ro) in ref_offsets.iter().enumerate() {
        let v = src_refs.get(k).copied().unwrap_or(Value::Null);
        ref_word::store(c, base + ro as usize, &v)?.with_barrier_value(&v, |s| barrier(s));
    }
    Ok(())
}

/// Visit the blob's reference edges (GC tracing — a boxed value yields its box).
#[inline]
pub fn visit(c: &Cells, base: usize, ref_offsets: &[u32], visit: &mut dyn FnMut(&Value)) {
    for &ro in ref_offsets {
        ref_word::visit(c, base + ro as usize, visit);
    }
}
