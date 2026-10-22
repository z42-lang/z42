//! **fix-missing-write-barriers **: re-derive the generational invariant from scratch.
//!
//! The invariant — **every old object or array that holds a young reference is in the remembered
//! set** (object model R6 / R9) — is what lets a minor root itself from the remembered set and
//! nothing else in the old generation (see `barrier.rs`). It has exactly two producers: the write
//! barrier on an old→young store and `remember_newly_old` on promotion. A store that skips the
//! barrier breaks it silently: nothing fails at the store, the young referent is swept at a later
//! minor, and the symptom is a use-after-free collections away from the cause.
//!
//! This check makes the break loud at the first minor after the store, naming the owner. It walks
//! every alive old object and array, so it costs O(old heap) — opt-in only:
//! `Z42_GC_VERIFY_CARDS=1` runs it before every minor; tests call it directly.
//!
//! The rest of the region (strings, closures, PRIM arrays) needs neither: strings and PRIM arrays
//! are leaves, and a closure's reference (`env`) is fixed at construction, so neither can *gain* a
//! young child after it is old.

use crate::gc::{ArrRef, GcMode, MagrGC, ObjRef};
use crate::gc::var_region::BlockType;
use crate::metadata::Value;

impl crate::gc::arc_heap::ArcMagrGC {
    /// Check the remembered-set invariant over the whole old generation. `Err` lists the
    /// offending owners (capped), each with the young child it holds. Always `Ok` outside
    /// generational mode.
    ///
    /// Skips the blocks a minor itself skips: doomed ones while an incremental major sweeps
    /// (unmarked = garbage the cursor has not reached; their children may be reclaimed already —
    /// reading them would follow dangling edges, see `doomed_unless_marked`).
    pub(crate) fn verify_card_invariant(&self) -> Result<(), String> {
        if self.mode() != GcMode::GenerationalMarkSweep {
            return Ok(());
        }
        let threshold = self.promotion_age();
        let doomed = self.doomed_unless_marked();
        let mut bad: Vec<String> = Vec::new();
        let mut owners: Vec<Value> = Vec::new();
        self.region_var.lock().iterate_alive(|h, header| {
            let bt = header.block_type();
            if bt.may_be_remembered()
                && header.gen_age() >= threshold
                && !header.is_remembered()
                && doomed.is_none_or(|k| header.is_marked(k))
            {
                owners.push(if bt == BlockType::Object {
                    Value::Object(ObjRef::from_var(h))
                } else {
                    Value::Array(ArrRef::from_var(h))
                });
            }
        });
        for owner in owners {
            if let Some(child) = Self::first_young_child(&owner, threshold) {
                match owner {
                    Value::Object(o) => {
                        let name = o.borrow().type_desc.name.to_string();
                        bad.push(format!("old object `{name}` holds young {child} but is not remembered"));
                    }
                    Value::Array(a) => {
                        let (et, len) = { let a = a.borrow(); (a.element_type(), a.len()) };
                        bad.push(format!("old array `{et}[{len}]` holds young {child} but is not remembered"));
                    }
                    _ => {}
                }
            }
        }
        if bad.is_empty() {
            return Ok(());
        }
        const SHOWN: usize = 8;
        let more = bad.len().saturating_sub(SHOWN);
        bad.truncate(SHOWN);
        Err(format!(
            "remembered-set invariant broken — a reference store skipped the write barrier \
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
