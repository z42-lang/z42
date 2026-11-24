//! Runtime package lookup borrows loading state and metadata, never a VM or executor.
use std::{path::PathBuf, sync::Arc};
use rustc_hash::FxHashMap;
use crate::{loader::lazy::{LazyLoader, ZpkgCandidate}, metadata::Function, object::TypeDesc};

pub struct RuntimeLoader<'a> {
    state: &'a super::LoaderState,
    metadata: &'a crate::metadata::RuntimeState,
    cache: &'a crate::metadata::ThreadRuntimeState,
    events: crate::diagnostics::ModuleLoadEvents<'a>,
}
impl<'a> RuntimeLoader<'a> {
    #[inline]
    pub(crate) fn new(
        state: &'a super::LoaderState, metadata: &'a crate::metadata::RuntimeState,
        cache: &'a crate::metadata::ThreadRuntimeState,
        events: crate::diagnostics::ModuleLoadEvents<'a>,
    ) -> Self { Self { state, metadata, cache, events } }

    pub(crate) fn configure_native(&self, settings: super::NativeLoading) -> anyhow::Result<()> {
        self.state.configure_native(settings)
    }

    pub(crate) fn native_loading(&self) -> &'a super::NativeLoading {
        self.state.native_loading()
    }
}
#[path = "runtime_contexts.rs"]
mod contexts;
#[path = "runtime_modules.rs"]
mod modules;

impl RuntimeLoader<'_> {

    /// An already-registered function, or `None`, without loading a package.
    /// Unlike [`Self::try_lookup_function`], this only reads current registrations. For callers that
    /// must not re-enter the VM, such as the JIT translator recognising intrinsics and
    /// the `[Native]` forward probe (`exec::call::forward`).
    pub fn probe_loaded_function(&self, func_name: &str) -> Option<Arc<Function>> {
        self.state.lazy_loader.read().as_ref()?.probe_function(func_name)
    }

    fn entry_type(&self, name: &str) -> Option<Arc<TypeDesc>> {
        self.metadata.module.as_ref()?.type_registry.get(name).cloned()
    }

    /// Resolve an exact name, preferring the entry module before probing packages.
    /// Package lookup can load dependencies and emit module-load events.
    pub(crate) fn resolve_type(&self, name: &str) -> Option<Arc<TypeDesc>> {
        self.entry_type(name)
            .map(|td| self.metadata.current_type_desc(td))
            .or_else(|| self.try_lookup_type(name))
    }

    /// Probe the entry module before loaded packages, without triggering a load.
    /// Also returns registered types whose base chain has yet to be merged.
    pub(crate) fn probe_loaded_type(&self, name: &str) -> Option<Arc<TypeDesc>> {
        if let Some(td) = self.entry_type(name) {
            return Some(td);
        }
        self.state.lazy_loader.read().as_ref()?.loaded_type(name)
    }

    pub fn try_lookup_function(&self, func_name: &str) -> Option<Arc<Function>> {
        // parallel-parse-and-cache：线程本地前置缓存，命中不碰共享锁（见 `fn_lookup_cache`）。
        if let Some(f) = self.cache.fn_lookup_cache.lock().get(func_name).cloned() {
            if !self.type_load_drain_is_noop(&[]) {
                self.drain_pending_type_loads();
            }
            return Some(f);
        }
        // fix-lazy-lookup-contention：稳态命中走读锁 —— 没有加载任何 zpkg，所以
        // `newly_loaded` 恒空；类型加载队列的排空判定仍照旧。
        //
        // fix-negative-cache-under-read-lock：**miss 也走读锁**。`resolve_function` 的第一件事
        // 就是查负缓存并直接返回 `None`（热点是无构造函数类合成的 `..ctor$0`，每次 `new` 查一次）——
        // 那是一次纯记忆化的「否」，却要付一次独占锁并堵住所有读者。这里在读锁下先答掉。
        let fast = {
            let state = self.state.lazy_loader.read();
            match state.as_ref() {
                Some(loader) => {
                    match loader.probe_function(func_name) {
                        Some(f) => Some(Some(f)),
                        None if loader.known_unresolved_function_ro(func_name) => Some(None),
                        None => None,
                    }
                }
                None => return None,
            }
        };
        // `resolved` 已经是最终答案（`Some` = 命中，`None` = 负缓存里的确定性「否」）。
        if let Some(resolved) = fast {
            if let Some(f) = &resolved { self.cache.fn_lookup_cache.lock().insert(func_name.into(), Arc::clone(f)); }
            if !self.type_load_drain_is_noop(&[]) {
                self.drain_pending_type_loads();
            }
            return resolved;
        }
        let (result, newly_loaded) = {
            let mut state = self.state.lazy_loader.write();
            let loader = state.as_mut()?;
            // reduce-lazy-lookup-alloc: drain the loader's `newly_loaded` scratch
            // buffer around the resolve, instead of cloning + diffing the whole
            // `loaded_zpkgs` set on every call (profiled: that per-call clone was
            // the top interp alloc hotspot in cross-zpkg-heavy workloads such as
            // z42c self-compile). Common path (no new zpkg loaded) → empty buffer
            // → `mem::take` of a zero-cap Vec → zero allocation.
            loader.newly_loaded.clear();
            let result = loader.resolve_function(func_name);
            let newly = std::mem::take(&mut loader.newly_loaded);
            (result, newly)
        };
        if self.type_load_drain_is_noop(&newly_loaded) {
            return result;
        }
        // Resolve queued type owners after releasing the loader lock.
        self.drain_pending_type_loads();
        for name in newly_loaded {
            self.events.loaded(name, None);
        }
        result
    }

    /// runtime-ambiguous-use-site: 这个函数名是否被两个已加载 zpkg 各自声明过。
    /// **调用方须先用 `lazy_loader::ambiguity_seen()` 挡一道**——本方法要拿读锁。
    pub fn is_ambiguous_function(&self, name: &str) -> bool {
        let state = self.state.lazy_loader.read();
        match state.as_ref() { Some(l) => l.is_ambiguous_function(name), None => false }
    }

    /// 类型侧孪生，见 [`Self::is_ambiguous_function`]。
    pub fn is_ambiguous_type(&self, name: &str) -> bool {
        let state = self.state.lazy_loader.read();
        match state.as_ref() { Some(l) => l.is_ambiguous_type(name), None => false }
    }

    /// The `TypeId` of the type the lazy loader has registered under `class_name`, without
    /// loading anything (`None` if no loader / not registered / no id). Type-test target keys
    /// (`exec::ops::cast::target_key`) use it.
    pub(crate) fn loaded_type_id(&self, class_name: &str) -> Option<crate::metadata::ids::tokens::TypeId> {
        let state = self.state.lazy_loader.read();
        let id = state.as_ref()?.loaded_type(class_name)?.id;
        id.is_resolved().then_some(id)
    }

    /// Look up a class TypeDesc by FQ name; triggers lazy load if needed.
    /// Same `ModuleLoaded` emit semantics as `try_lookup_function`.
    pub fn try_lookup_type(&self, class_name: &str) -> Option<Arc<TypeDesc>> {
        // parallel-parse-and-cache：线程本地前置缓存，命中不碰共享锁（见 `type_lookup_cache`）。
        let cached = self.cache.type_lookup_cache.lock().get(class_name).cloned();
        if let Some(td) = cached {
            if !self.type_load_drain_is_noop(&[]) {
                self.drain_pending_type_loads();
            }
            return Some(self.metadata.current_type_desc(td));
        }
        // fix-lazy-lookup-contention：同 try_lookup_function —— registry 命中且基类链完整
        // 时是纯读，走读锁。
        // fix-negative-cache-under-read-lock：同 try_lookup_function —— 已知解析不出来的类名
        // （`obj_new` 每次分配都要探一次）也在读锁下答掉，别为一个记忆化的「否」拿独占锁。
        let fast = {
            let state = self.state.lazy_loader.read();
            match state.as_ref() {
                Some(loader) => {
                    match loader.probe_type(class_name) {
                        Some(td) => Some(Some(td)),
                        None if loader.known_unresolved_type_ro(class_name) => Some(None),
                        None => None,
                    }
                }
                None => return None,
            }
        };
        // 同上：`resolved` 即最终答案。
        if let Some(resolved) = fast {
            // 只缓存基类链已合并的完整描述符（`probe_type` 只交出完整的，这里再守一道）。
            if let Some(td) = &resolved {
                if !td.base_unmerged() { self.cache.type_lookup_cache.lock().insert(class_name.into(), Arc::clone(td)); }
            }
            if !self.type_load_drain_is_noop(&[]) {
                self.drain_pending_type_loads();
            }
            return resolved.map(|td| self.metadata.current_type_desc(td));
        }
        let (result, newly_loaded) = {
            let mut state = self.state.lazy_loader.write();
            let loader = state.as_mut()?;
            // reduce-lazy-lookup-alloc: drain scratch buffer (see try_lookup_function).
            loader.newly_loaded.clear();
            let result = loader.resolve_type(class_name);
            let newly = std::mem::take(&mut loader.newly_loaded);
            (result, newly)
        };
        if self.type_load_drain_is_noop(&newly_loaded) {
            return result.map(|td| self.metadata.current_type_desc(td));
        }
        // defer-class-initialization (T2): 同上。
        self.drain_pending_type_loads();
        for name in newly_loaded {
            self.events.loaded(name, None);
        }
        result.map(|td| self.metadata.current_type_desc(td))
    }

    /// No packages or queued type owners need the drain path. A batch being
    /// detached by another thread prevents caching an incomplete lookup result.
    #[inline]
    pub fn type_load_drain_is_noop(&self, newly_loaded: &[String]) -> bool {
        use std::sync::atomic::Ordering;
        newly_loaded.is_empty()
            && self.state.pending_type_load_count.load(Ordering::Relaxed) == 0
            && self.state.type_load_batch_inflight.load(Ordering::Acquire) == 0
    }
    /// Resolve queued type owners and load their packages without running managed
    /// initializers. Package/type initialization remains the execution barriers' job.
    /// A recursive lookup leaves any newly queued work to the outer drain loop.
    pub fn drain_pending_type_loads(&self) {
        thread_local! {
            static DRAINING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) { DRAINING.with(|d| d.set(false)); }
        }
        if DRAINING.with(|d| d.replace(true)) {
            return; // 已在排空中（嵌套触发）——外层循环会处理新入队项
        }
        let _guard = Guard;

        loop {
            // 取队列**之前**先宣告在飞：从这一刻起到本批处理完，别的线程都不会把
            // 「队列与计数都是零」误读成「已静止」（见 `type_load_batch_inflight`）。
            self.state.type_load_batch_inflight.fetch_add(1, std::sync::atomic::Ordering::Release);
            let types: Vec<String> = {
                let mut q = self.state.pending_type_loads.lock();
                self.state.pending_type_load_count
                    .store(0, std::sync::atomic::Ordering::Relaxed);
                std::mem::take(&mut *q)
            };
            self.state.type_load_batch_inflight.fetch_sub(1, std::sync::atomic::Ordering::Release);
            if types.is_empty() { return; }
            for class_fq in &types {
                let _ = self.try_lookup_type(class_fq);
            }
        }
    }


    /// Whether the lazy registry already contains this loaded type.
    /// 只读，不触发任何加载——供 T3 入队前的快速过滤。
    pub fn has_loaded_type(&self, class_fq: &str) -> bool {
        let state = self.state.lazy_loader.read();
        match state.as_ref() {
            Some(loader) => loader.has_type(class_fq),
            None => false,
        }
    }

    /// Queue a static field owner for package loading during token preparation.
    /// This registers initialization facts; it does not execute its initializer.
    pub fn enqueue_type_load(&self, class_fq: &str) {
        let mut q = self.state.pending_type_loads.lock();
        if !q.iter().any(|c| c == class_fq) {
            q.push(class_fq.to_string());
            self.state.pending_type_load_count
                .store(q.len(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Force-load every not-yet-loaded declared package (one-time eager load).
    /// Used by reflection's `make_type_from_name` dotless fallback when a class-like
    /// simple name (e.g. a constructed-generic field type_tag base `List`) can't be
    /// resolved from already-loaded types. See `LazyLoader::force_load_all`.
    pub fn force_load_all_packages(&self) {
        let newly = {
            let mut state = self.state.lazy_loader.write();
            match state.as_mut() {
                Some(loader) => {
                    loader.newly_loaded.clear();
                    loader.force_load_all();
                    std::mem::take(&mut loader.newly_loaded)
                }
                None => Vec::new(),
            }
        };
        // Reflection may load multiple packages. Drain any queued type owners
        // before publishing their load events; do not execute their initializers.
        self.drain_pending_type_loads();
        for name in newly {
            self.events.loaded(name, None);
        }
    }

    /// add-crosspkg-impl-reflection (unify P1-e): traits added to `target_fq`
    /// via cross-package `impl Trait for Type`, aggregated from every loaded
    /// package's IMPL section (returns owned Vec — the registry lives behind
    /// the lazy-loader lock). Empty when no loader / no impls.
    pub fn impl_traits_for(&self, target_fq: &str) -> Vec<String> {
        let state = self.state.lazy_loader.read();
        match state.as_ref() {
            Some(loader) => loader.impl_traits_for(target_fq).to_vec(),
            None => Vec::new(),
        }
    }

    /// All currently-loaded type names — entry module's `type_registry` plus the
    /// lazy loader's already-loaded types. add-nested-types: `GetNestedTypes`
    /// scans these for `<outer>+<simple>` members. A type's nested types live in
    /// the same package as their declaring type, so resolving the declaring type
    /// (its handle is in hand) has already force-loaded the package — no need to
    /// force-load everything. Deduped across both sources.
    pub fn loaded_type_names(&self) -> Vec<String> {
        let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(m) = self.metadata.module.as_ref() {
            for name in m.type_registry.keys() {
                set.insert(name.clone());
            }
        }
        {
            let state = self.state.lazy_loader.read();
            if let Some(loader) = state.as_ref() {
                for name in loader.iter_type_names() {
                    set.insert(name.clone());
                }
            }
        }
        set.into_iter().collect()
    }

    /// The unique loaded full name matching a short name. Identical full names
    /// in the entry module and lazy loader count once, as in loaded_type_names.
    pub fn unique_loaded_full_name(&self, simple: &str) -> Option<String> {
        let state = self.state.lazy_loader.read();
        let entry_names = self.metadata.module.as_ref().into_iter().flat_map(|module| module.type_registry.keys());
        let lazy_names = state.as_ref().into_iter().flat_map(|loader| loader.iter_type_names());
        let mut matches = entry_names.chain(lazy_names)
            .filter(|name| crate::zpkg::index::type_names::simple_matches(name, simple));
        let first = matches.next()?;
        if matches.any(|name| name != first) {
            return None;
        }
        Some(first.clone())
    }

    /// All namespaces declared by lazy-loadable zpkgs (for static-init scan).
    pub fn declared_namespaces(&self) -> Vec<String> {
        let state = self.state.lazy_loader.read();
        match state.as_ref() {
            Some(loader) => loader.declared_namespaces(),
            None         => Vec::new(),
        }
    }

}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
