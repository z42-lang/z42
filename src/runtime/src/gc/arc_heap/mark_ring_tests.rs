//! The ring is a FIFO of `RING` values: nothing comes out until it is full, then the oldest.

use super::*;

#[test]
fn values_leave_in_order_after_ring_others_entered() {
    let mut r = MarkRing::new();
    let mut out = Vec::new();
    for i in 0..(RING as i64 + 5) {
        if let Some(Value::I64(x)) = r.push(Value::I64(i)) {
            out.push(x);
        }
    }
    assert_eq!(out, (0..5).collect::<Vec<_>>(), "the ring holds RING values before releasing any");
    while let Some(Value::I64(x)) = r.pop_oldest() {
        out.push(x);
    }
    assert_eq!(out, (0..RING as i64 + 5).collect::<Vec<_>>(), "draining yields the rest, oldest first");
    assert!(r.pop_oldest().is_none());
}
