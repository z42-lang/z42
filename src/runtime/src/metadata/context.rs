//! Load-context model — the runtime code-boundary abstraction (dotnet
//! `AssemblyLoadContext` equivalent, Phase 1 地基).
//!
//! z42 previously had **no runtime code boundary**: `merge::merge_modules`
//! collapses every zpkg into one flat `Module`. This module introduces the
//! boundary so code can be grouped into contexts, its zpkg identity preserved
//! at runtime as an `Assembly`, and (in later changes) unloaded / hot-reloaded.
//!
//! **Phase 1 scope (add-load-context-model): boundary + identity only.**
//! - `root` context (id 0) — core/stdlib/main program;永驻, not collectible.
//!   Keeps the existing flat merged `Module` + O(1) MethodId dispatch (unchanged).
//! - `collectible` contexts — created on demand, each owns its loaded assemblies
//!   in a private arena. Reflection-visible; **not yet executable / unloadable**.
//!
//! Design: `docs/spec/changes/add-load-context-model/design.md` (D1–D6).
//! The context/assembly association lives HERE (registry) + on the `Std.Type`
//! object's `__asmId` slot — TypeDesc itself is not mutated (D5 refinement).

use std::sync::Arc;

use super::bytecode::Module;
use super::types::TypeDesc;

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

/// One load context: a name, whether it is collectible, and the assemblies
/// loaded into it.
#[derive(Debug)]
struct ContextEntry {
    name: String,
    is_collectible: bool,
    assemblies: Vec<AssemblyId>,
}

/// One loaded assembly: a name, its owning context, and (for collectible loads)
/// the private `Module` arena holding its parsed code + metadata. The root
/// assembly has `module = None` (its code lives in the shared merged `Module`).
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
                assemblies: vec![AssemblyId::ROOT],
            }],
            assemblies: vec![AssemblyEntry {
                name: "root".to_string(),
                context: ContextId::ROOT,
                module: None,
            }],
        }
    }

    /// Create a new collectible context, returning its id.
    pub fn create_collectible(&mut self, name: &str) -> ContextId {
        let id = ContextId(self.contexts.len() as u32);
        self.contexts.push(ContextEntry {
            name: name.to_string(),
            is_collectible: true,
            assemblies: Vec::new(),
        });
        id
    }

    /// Register a loaded module as an assembly in `ctx`, returning its id.
    /// Does not validate `ctx` beyond bounds — callers hold a live handle.
    pub fn load_into(&mut self, ctx: ContextId, name: &str, module: Module) -> AssemblyId {
        let aid = AssemblyId(self.assemblies.len() as u32);
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
    /// the root assembly (its module is `None` — root reflection goes through
    /// the ordinary `typeof` / registry path, not `Assembly.GetTypes`).
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
