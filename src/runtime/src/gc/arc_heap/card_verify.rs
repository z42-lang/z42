//! **fix-missing-write-barriers (2026-10-07)**: re-derive the card-table invariant from scratch.
//!
//! The invariant — **every old entry that holds a young reference sits on a dirty card** — is
//! what lets a minor root itself from the dirty cards and nothing else in the old generation
//! (see `barrier.rs`). It has exactly two producers: the write barrier on an old→young store and
//! `dirty_cards_for_newly_old_*` on promotion. A store that skips the barrier breaks it silently:
//! nothing fails at the store, the young referent is swept at a later minor, and the symptom is a
//! use-after-free collections away from the cause.
//!
//! This check makes the break loud at the first minor after the store, naming the owner. It walks
//! every alive old entry of the two carded regions, so it costs O(old heap) — opt-in only:
//! `Z42_GC_VERIFY_CARDS=1` runs it before every minor; tests call it directly.
//!
//! `region_var` (strings, closures) has no card table and needs none: strings are leaves, and a
//! closure's references (`env`, `fn_name`) are fixed at construction, so neither can *gain* a young
//! child after it is old.

use crate::gc::refs::GcRef;
use crate::gc::{GcMode, MagrGC};
use crate::metadata::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// Check the card-table invariant over the whole old generation. `Err` lists the offending
    /// owners (capped), each with the young child it holds. Always `Ok` outside generational mode
    /// (no cards to check).
    ///
    /// Skips the entries a minor itself skips: doomed ones while an incremental major sweeps
    /// (unmarked = garbage the cursor has not reached; their children may be reclaimed already —
    /// reading them would follow dangling edges, see `doomed_unless_marked`).
    pub(crate) fn verify_card_invariant(&self) -> Result<(), String> {
        if self.mode() != GcMode::GenerationalMarkSweep {
            return Ok(());
        }
        let threshold = self.promotion_age();
        let doomed = self.doomed_unless_marked();
        let mut bad: Vec<String> = Vec::new();
        {
            let region = self.region_object.lock();
            region.iterate_alive(|h, entry| {
                if entry.gen_age.load(std::sync::atomic::Ordering::Relaxed) < threshold
                    || doomed.is_some_and(|k| !entry.is_marked(k))
                    || region.is_entry_card_dirty(h.chunk_idx, h.entry_idx)
                {
                    return;
                }
                // SAFETY: handle from `iterate_alive` under the region lock; the entry is alive and
                // its generation matches.
                let gc = unsafe { GcRef::from_region_entry(std::ptr::NonNull::from(entry), h.generation) };
                let owner = Value::Object(gc);
                if let Some(child) = Self::first_young_child(&owner, threshold) {
                    let name = gc.borrow().type_desc.name.to_string();
                    bad.push(format!("old object `{name}` holds young {child} on a clean card"));
                }
            });
        }
        {
            let region = self.region_array.lock();
            region.iterate_alive(|h, entry| {
                if entry.gen_age.load(std::sync::atomic::Ordering::Relaxed) < threshold
                    || doomed.is_some_and(|k| !entry.is_marked(k))
                    || region.is_entry_card_dirty(h.chunk_idx, h.entry_idx)
                {
                    return;
                }
                // SAFETY: as above.
                let gc = unsafe { GcRef::from_region_entry(std::ptr::NonNull::from(entry), h.generation) };
                let owner = Value::Array(gc);
                if let Some(child) = Self::first_young_child(&owner, threshold) {
                    let len = gc.borrow().len();
                    bad.push(format!("old array (len {len}) holds young {child} on a clean card"));
                }
            });
        }
        if bad.is_empty() {
            return Ok(());
        }
        const SHOWN: usize = 8;
        let more = bad.len().saturating_sub(SHOWN);
        bad.truncate(SHOWN);
        Err(format!(
            "card-table invariant broken — a reference store skipped the write barrier \
             (route it through objops / `write_barrier_*`):\n  {}{}",
            bad.join("\n  "),
            if more > 0 { format!("\n  … and {more} more") } else { String::new() },
        ))
    }

    /// The first young heap child of `owner`, described for the report.
    fn first_young_child(owner: &Value, threshold: u8) -> Option<String> {
        let mut found: Option<String> = None;
        owner.visit_gc_children(None, &mut |child| {
            if found.is_none() && child.is_heap_ref() && Self::gen_age_of(child) < threshold {
                found = Some(format!(
                    "{} (age {})",
                    crate::semantics::value_kind_name(child),
                    Self::gen_age_of(child),
                ));
            }
        });
        found
    }

    /// `Z42_GC_VERIFY_CARDS`: the minor's entry check. A broken invariant is a VM bug about to
    /// become a use-after-free, so it panics right here rather than letting the minor sweep.
    #[inline]
    pub(super) fn maybe_verify_cards_before_minor(&self) {
        if !crate::config::runtime_config().gc_verify_cards {
            return;
        }
        if let Err(msg) = self.verify_card_invariant() {
            panic!("Z42_GC_VERIFY_CARDS: {msg}");
        }
    }
}
