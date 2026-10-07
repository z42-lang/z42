use super::*;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn locate_maps_segment_boundaries() {
    assert_eq!(locate(0), Some((0, 0)));
    assert_eq!(locate(FIRST_SEG - 1), Some((0, FIRST_SEG - 1)));
    assert_eq!(locate(FIRST_SEG), Some((1, 0)));
    assert_eq!(locate(3 * FIRST_SEG - 1), Some((1, 2 * FIRST_SEG - 1)));
    assert_eq!(locate(3 * FIRST_SEG), Some((2, 0)));
    assert_eq!(locate(SEG_CAPACITY - 1), Some((NSEG - 1, seg_len(NSEG - 1) - 1)));
    assert_eq!(locate(SEG_CAPACITY), None);
    assert!(SEG_CAPACITY as u64 <= crate::metadata::tokens::IMPORT_BASE as u64);
}

#[test]
fn push_and_get_across_segments() {
    let v: SegVec<usize> = SegVec::new();
    assert!(v.is_empty());
    assert!(v.get(0).is_none());
    let n = 3 * FIRST_SEG + 7; // spans three segments
    for i in 0..n {
        assert_eq!(v.push_with(|idx| idx * 2), Some(i));
    }
    assert_eq!(v.len(), n);
    for i in 0..n {
        assert_eq!(v.get(i), Some(&(i * 2)));
    }
    assert!(v.get(n).is_none());
}

#[test]
fn references_stay_valid_while_the_vec_grows() {
    let v: SegVec<String> = SegVec::new();
    v.push_with(|_| "first".to_string());
    let first: &String = v.get(0).unwrap();
    for i in 0..(4 * FIRST_SEG) {
        v.push_with(|_| i.to_string());
    }
    // Segments never move: the early reference is still the same element.
    assert_eq!(first, "first");
    assert!(std::ptr::eq(first, v.get(0).unwrap()));
}

#[test]
fn extend_with_publishes_a_block() {
    let v: SegVec<u32> = SegVec::new();
    v.push_with(|_| 100);
    let first = v.extend_with(FIRST_SEG + 5, |i| i as u32).unwrap();
    assert_eq!(first, 1);
    assert_eq!(v.len(), FIRST_SEG + 6);
    assert_eq!(v.get(1), Some(&1));
    assert_eq!(v.get(FIRST_SEG + 5), Some(&((FIRST_SEG + 5) as u32)));
    assert_eq!(v.extend_with(0, |_| 0), Some(FIRST_SEG + 6));
    assert!(v.extend_with(SEG_CAPACITY, |_| 0).is_none(), "overflowing extend publishes nothing");
    assert_eq!(v.len(), FIRST_SEG + 6);
}

/// Counts drops so `Drop` is checked to run exactly once per published element.
struct DropCounter(Arc<AtomicUsize>);
impl Drop for DropCounter {
    fn drop(&mut self) { self.0.fetch_add(1, Ordering::Relaxed); }
}

#[test]
fn drop_runs_once_per_element() {
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let v: SegVec<DropCounter> = SegVec::new();
        for _ in 0..(FIRST_SEG + 3) {
            v.push_with(|_| DropCounter(Arc::clone(&drops)));
        }
    }
    assert_eq!(drops.load(Ordering::Relaxed), FIRST_SEG + 3);
}

/// Readers spin on `len()` while two appenders grow the vec past several
/// segment boundaries. Every published index must read back the value its
/// writer stored (`index * 3 + 1`) — never a torn / uninitialized slot.
#[test]
fn concurrent_readers_see_only_published_elements() {
    const PER_WRITER: usize = 6 * FIRST_SEG;
    let v: Arc<SegVec<(usize, u64)>> = Arc::new(SegVec::new());
    let done = Arc::new(AtomicUsize::new(0));
    let checked = Arc::new(AtomicU64::new(0));

    let writers: Vec<_> = (0..2)
        .map(|_| {
            let v = Arc::clone(&v);
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                for _ in 0..PER_WRITER {
                    v.push_with(|i| (i, i as u64 * 3 + 1)).unwrap();
                }
                done.fetch_add(1, Ordering::Release);
            })
        })
        .collect();
    let readers: Vec<_> = (0..3)
        .map(|r| {
            let v = Arc::clone(&v);
            let done = Arc::clone(&done);
            let checked = Arc::clone(&checked);
            std::thread::spawn(move || loop {
                let finished = done.load(Ordering::Acquire) == 2;
                let len = v.len();
                // Probe the newest element plus a stride over the rest.
                for i in (r..len).step_by(97).chain(len.checked_sub(1)) {
                    let &(idx, val) = v.get(i).expect("index below len must be readable");
                    assert_eq!(idx, i);
                    assert_eq!(val, i as u64 * 3 + 1);
                    checked.fetch_add(1, Ordering::Relaxed);
                }
                if finished { break; }
            })
        })
        .collect();
    for t in writers.into_iter().chain(readers) {
        t.join().unwrap();
    }
    assert_eq!(v.len(), 2 * PER_WRITER);
    assert!(checked.load(Ordering::Relaxed) > 0);
}

#[test]
fn sparse_table_allocates_on_first_touch() {
    let t: SparseSegTable<AtomicU64> = SparseSegTable::new();
    assert!(t.get(5).is_none(), "untouched segment reads as absent");
    t.get_or_init(5).unwrap().store(42, Ordering::Relaxed);
    assert_eq!(t.get(5).unwrap().load(Ordering::Relaxed), 42);
    // Same segment, other entry: present and defaulted.
    assert_eq!(t.get(6).unwrap().load(Ordering::Relaxed), 0);
    // A far segment stays unallocated until touched.
    let far = 100 * FIRST_SEG;
    assert!(t.get(far).is_none());
    assert_eq!(t.get_or_init(far).unwrap().load(Ordering::Relaxed), 0);
    assert!(t.get_or_init(SEG_CAPACITY).is_none());
}

#[test]
fn sparse_table_concurrent_first_touch_converges_on_one_segment() {
    let t: Arc<SparseSegTable<AtomicU64>> = Arc::new(SparseSegTable::new());
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8u64)
        .map(|n| {
            let t = Arc::clone(&t);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                // All threads race to install segment 1, each bumping its own entry.
                for i in 0..FIRST_SEG {
                    t.get_or_init(FIRST_SEG + i).unwrap().fetch_add(n + 1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    for th in threads { th.join().unwrap(); }
    // Every increment landed in the one installed segment (losers' segments were discarded
    // before any of their entries were handed out): 1 + 2 + … + 8 = 36 per entry.
    for i in 0..FIRST_SEG {
        assert_eq!(t.get(FIRST_SEG + i).unwrap().load(Ordering::Relaxed), 36);
    }
}

#[test]
fn sparse_table_drops_installed_entries() {
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let t: SparseSegTable<std::sync::OnceLock<DropCounter>> = SparseSegTable::new();
        t.get_or_init(3).unwrap().get_or_init(|| DropCounter(Arc::clone(&drops)));
        t.get_or_init(5000).unwrap().get_or_init(|| DropCounter(Arc::clone(&drops)));
    }
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}
