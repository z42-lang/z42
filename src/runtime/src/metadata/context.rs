//! Load-context model — the runtime code-boundary abstraction (dotnet
//! `AssemblyLoadContext` equivalent).
//!
//! z42 previously had **no runtime code boundary**: `merge::merge_modules`
//! collapses every zpkg into one flat `Module`. This module introduces the
//! boundary so code can be grouped into contexts, its zpkg identity preserved
//! at runtime as an `Assembly`, and unloaded / hot-reloaded.
//!
//! - `root` context (id 0) — core/stdlib/main program;永驻, not collectible.
//!   Keeps the existing flat merged `Module` + O(1) MethodId dispatch (unchanged).
//! - `collectible` contexts — created on demand, each owns its loaded assemblies
//!   in a private arena. Reflection-visible.
//!
//! **Phase 2 (add-lazy-context-unload): 惰性卸载.** `Unload()` marks a
//! collectible context `Unloading`; GC mark tags which contexts are still
//! referenced by live objects (instance `type_desc` + reflection native
//! handles), and a post-sweep reclaim pass drops the arena of any `Unloading`
//! context with no live references (Erlang current/old — no tombstone).
//!
//! Design: `docs/spec/changes/add-lazy-context-unload/design.md` (D1–D5).
//! Context↔type association lives HERE (registry `td_to_ctx` reverse map),
//! never mutating `TypeDesc` (Phase 1 D5).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::bytecode::Module;
use super::types::{NativeData, TypeDesc};

/// Identifies one load context in [`ContextRegistry`]. `ContextId(0)` is always
/// the永驻 root. Stable for the lifetime of one `VmContext`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ContextId(pub u32);

impl ContextId {
    /// The永驻 root context — core/stdlib/main program. Never collectible.
    pub const ROOT: ContextId = ContextId(0);
}

/// Identifies one loaded assembly (zpkg runtime projection) in
/// [`ContextRegistry`]. `AssemblyId(0)` is the synthetic root assembly (the
/// merged root `Module`); it carries no owned arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct AssemblyId(pub u32);

impl AssemblyId {
    /// The synthetic root assembly — every `typeof(T)` for a root type reports
    /// this. Not collectible; owns no private arena.
    pub const ROOT: AssemblyId = AssemblyId(0);
}

/// Lifecycle state of a load context (add-lazy-context-unload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextState {
    /// Normal — accepts `Load`, resolvable.
    Active,
    /// `Unload()` called — no new `Load`; awaiting GC reclamation.
    Unloading,
    /// Arena freed — the context's code/metadata is gone.
    Reclaimed,
}

/// Outcome of [`ContextRegistry::unload`], mapped to a z42 result by the builtin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnloadOutcome {
    /// Transitioned Active → Unloading.
    Marked,
    /// Already Unloading / Reclaimed — no-op (idempotent).
    AlreadyUnloading,
    /// Root context — refused (never collectible).
    RootRejected,
    /// Unknown context id.
    NotFound,
}

/// One load context: a name, whether it is collectible, its lifecycle state,
/// and the assemblies loaded into it.
#[derive(Debug)]
struct ContextEntry {
    name: String,
    is_collectible: bool,
    state: ContextState,
    assemblies: Vec<AssemblyId>,
}

/// One loaded assembly: a name, its owning context, and (for collectible loads)
/// the private `Module` arena holding its parsed code + metadata. The root
/// assembly has `module = None`. A reclaimed collectible assembly also has
/// `module = None` (arena freed) — the id slot is kept so `AssemblyId`s stay stable.
#[derive(Debug)]
struct AssemblyEntry {
    name: String,
    context: ContextId,
    module: Option<Module>,
}

/// Per-`VmCore` registry of load contexts and assemblies. Guarded by a
/// `Mutex` on `VmCore`. Root context (id 0) + root assembly (id 0) are
/// pre-populated by [`ContextRegistry::new`].
#[derive(Debug)]
pub struct ContextRegistry {
    contexts: Vec<ContextEntry>,
    assemblies: Vec<AssemblyEntry>,
    /// Reverse map: collectible `*const TypeDesc` (as usize) → owning context.
    /// Built at `load_into` for collectible contexts (root types NOT registered);
    /// cleared on reclaim. Lets GC resolve a live object's context without
    /// mutating `TypeDesc` (Phase 1 D5 / Phase 2 D1).
    td_to_ctx: HashMap<usize, ContextId>,
    /// Number of contexts in `Unloading` state. GC gates its per-object liveness
    /// hook on this being > 0 (zero-cost when no unload is in flight). `Arc` so
    /// the GC can read it cheaply without locking the registry.
    unloading_count: Arc<AtomicUsize>,
}

/// Snapshot of the context↔object association, taken once per GC collect (only
/// when an unload is in flight) so the mark loop can resolve object→context
/// without locking the registry per object.
#[derive(Debug, Default)]
pub struct ContextLiveness {
    td_to_ctx: HashMap<usize, ContextId>,
    asm_to_ctx: HashMap<u32, ContextId>,
    collectible: HashSet<u32>,
}

impl ContextLiveness {
    /// Which collectible context (if any) a live `ScriptObject` retains — via
    /// its instance `type_desc` ptr, or its reflection native handle
    /// (`TypeHandle`/`AssemblyHandle`/`LoadContextHandle`). Returns `None` for
    /// root/unrelated objects. (Phase 2 D4: reflection handles are retention
    /// edges too — a `Std.Type` of a collectible type pins it via `TypeHandle`.)
    pub fn retained_context(&self, type_desc_ptr: usize, native: &NativeData) -> Option<ContextId> {
        if let Some(&c) = self.td_to_ctx.get(&type_desc_ptr) {
            return Some(c);
        }
        match native {
            NativeData::TypeHandle(td) => {
                self.td_to_ctx.get(&(Arc::as_ptr(td) as usize)).copied()
            }
            NativeData::AssemblyHandle(aid) => self.asm_to_ctx.get(&aid.0).copied(),
            NativeData::LoadContextHandle(cid) => {
                self.collectible.contains(&cid.0).then_some(*cid)
            }
            _ => None,
        }
    }

    /// True when there is nothing collectible to track (skip the hook).
    pub fn is_empty(&self) -> bool {
        self.td_to_ctx.is_empty() && self.collectible.is_empty()
    }
}

impl Default for ContextRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextRegistry {
    /// Fresh registry with the root context + root assembly pre-populated.
    pub fn new() -> Self {
        Self {
            contexts: vec![ContextEntry {
                name: "root".to_string(),
                is_collectible: false,
                state: ContextState::Active,
                assemblies: vec![AssemblyId::ROOT],
            }],
            assemblies: vec![AssemblyEntry {
                name: "root".to_string(),
                context: ContextId::ROOT,
                module: None,
            }],
            td_to_ctx: HashMap::new(),
            unloading_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Shared unloading-count flag for the GC to read without locking (clone
    /// the `Arc` once at VM setup).
    pub fn unloading_flag(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.unloading_count)
    }

    /// Create a new collectible context, returning its id.
    pub fn create_collectible(&mut self, name: &str) -> ContextId {
        let id = ContextId(self.contexts.len() as u32);
        self.contexts.push(ContextEntry {
            name: name.to_string(),
            is_collectible: true,
            state: ContextState::Active,
            assemblies: Vec::new(),
        });
        id
    }

    /// Register a loaded module as an assembly in `ctx`, returning its id.
    /// For a collectible `ctx`, registers each `TypeDesc` ptr in `td_to_ctx`.
    /// Callers must reject `Load` into a non-`Active` context beforehand.
    pub fn load_into(&mut self, ctx: ContextId, name: &str, module: Module) -> AssemblyId {
        let aid = AssemblyId(self.assemblies.len() as u32);
        if ctx != ContextId::ROOT {
            for td in module.type_registry.values() {
                self.td_to_ctx.insert(Arc::as_ptr(td) as usize, ctx);
            }
        }
        self.assemblies.push(AssemblyEntry {
            name: name.to_string(),
            context: ctx,
            module: Some(module),
        });
        if let Some(c) = self.contexts.get_mut(ctx.0 as usize) {
            c.assemblies.push(aid);
        }
        aid
    }

    /// The lifecycle state of `ctx`, or `None` if unknown.
    pub fn context_state(&self, ctx: ContextId) -> Option<ContextState> {
        self.contexts.get(ctx.0 as usize).map(|c| c.state)
    }

    /// Mark a collectible context for unloading (idempotent). Root is refused.
    pub fn unload(&mut self, ctx: ContextId) -> UnloadOutcome {
        if ctx == ContextId::ROOT {
            return UnloadOutcome::RootRejected;
        }
        let Some(entry) = self.contexts.get_mut(ctx.0 as usize) else {
            return UnloadOutcome::NotFound;
        };
        match entry.state {
            ContextState::Active => {
                entry.state = ContextState::Unloading;
                self.unloading_count.fetch_add(1, Ordering::Relaxed);
                UnloadOutcome::Marked
            }
            ContextState::Unloading | ContextState::Reclaimed => UnloadOutcome::AlreadyUnloading,
        }
    }

    /// Take a liveness snapshot for the GC mark loop. Only maps collectible
    /// contexts; cheap to build (few collectible types). Empty when nothing is
    /// collectible.
    pub fn liveness_snapshot(&self) -> ContextLiveness {
        let mut asm_to_ctx = HashMap::new();
        let mut collectible = HashSet::new();
        for (i, c) in self.contexts.iter().enumerate() {
            if c.is_collectible && c.state != ContextState::Reclaimed {
                collectible.insert(i as u32);
            }
        }
        for (i, a) in self.assemblies.iter().enumerate() {
            if collectible.contains(&a.context.0) {
                asm_to_ctx.insert(i as u32, a.context);
            }
        }
        ContextLiveness {
            td_to_ctx: self.td_to_ctx.clone(),
            asm_to_ctx,
            collectible,
        }
    }

    /// Reclaim every `Unloading` context NOT in `live`: drop its assemblies'
    /// `Module` arenas (Arc refcount → 0, deterministic free), purge its
    /// `td_to_ctx` entries, mark `Reclaimed`, and decrement `unloading_count`.
    /// Returns the number of contexts reclaimed. Called at GC collect end
    /// (post-sweep, inside STW).
    pub fn reclaim(&mut self, live: &HashSet<ContextId>) -> usize {
        let to_reclaim: Vec<u32> = self
            .contexts
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                c.state == ContextState::Unloading && !live.contains(&ContextId(*i as u32))
            })
            .map(|(i, _)| i as u32)
            .collect();

        for cid in &to_reclaim {
            let ctx = ContextId(*cid);
            // Free each assembly's module + purge its TypeDesc ptrs.
            let aids: Vec<AssemblyId> = self
                .contexts
                .get(*cid as usize)
                .map(|c| c.assemblies.clone())
                .unwrap_or_default();
            for aid in aids {
                if let Some(entry) = self.assemblies.get_mut(aid.0 as usize) {
                    if let Some(module) = entry.module.take() {
                        for td in module.type_registry.values() {
                            self.td_to_ctx.remove(&(Arc::as_ptr(td) as usize));
                        }
                        // `module` drops here → Arc<TypeDesc>/functions/strings freed.
                    }
                }
            }
            if let Some(c) = self.contexts.get_mut(*cid as usize) {
                c.state = ContextState::Reclaimed;
            }
            self.unloading_count.fetch_sub(1, Ordering::Relaxed);
            let _ = ctx;
        }
        to_reclaim.len()
    }

    // ── Context accessors ───────────────────────────────────────────────────

    pub fn context_exists(&self, ctx: ContextId) -> bool {
        (ctx.0 as usize) < self.contexts.len()
    }

    pub fn context_name(&self, ctx: ContextId) -> Option<String> {
        self.contexts.get(ctx.0 as usize).map(|c| c.name.clone())
    }

    pub fn context_is_collectible(&self, ctx: ContextId) -> Option<bool> {
        self.contexts.get(ctx.0 as usize).map(|c| c.is_collectible)
    }

    pub fn context_assemblies(&self, ctx: ContextId) -> Vec<AssemblyId> {
        self.contexts
            .get(ctx.0 as usize)
            .map(|c| c.assemblies.clone())
            .unwrap_or_default()
    }

    // ── Assembly accessors ──────────────────────────────────────────────────

    pub fn assembly_name(&self, aid: AssemblyId) -> Option<String> {
        self.assemblies.get(aid.0 as usize).map(|a| a.name.clone())
    }

    pub fn assembly_context(&self, aid: AssemblyId) -> Option<ContextId> {
        self.assemblies.get(aid.0 as usize).map(|a| a.context)
    }

    /// Whether the assembly's owning context is collectible. `false` for the
    /// root assembly and any unknown id (degrade to not-collectible).
    pub fn assembly_is_collectible(&self, aid: AssemblyId) -> bool {
        self.assemblies
            .get(aid.0 as usize)
            .and_then(|a| self.context_is_collectible(a.context))
            .unwrap_or(false)
    }

    /// The `Arc<TypeDesc>`s defined by this assembly, sorted by FQ name for a
    /// stable, deterministic `GetTypes()` order (common-pitfalls §1). Empty for
    /// the root assembly and reclaimed assemblies (module = `None`).
    pub fn assembly_types(&self, aid: AssemblyId) -> Vec<Arc<TypeDesc>> {
        let Some(entry) = self.assemblies.get(aid.0 as usize) else {
            return Vec::new();
        };
        let Some(module) = entry.module.as_ref() else {
            return Vec::new();
        };
        let mut types: Vec<Arc<TypeDesc>> = module.type_registry.values().cloned().collect();
        types.sort_by(|a, b| a.name.cmp(&b.name));
        types
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod context_tests;
