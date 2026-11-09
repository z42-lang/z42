//! Owned Rust state registered before VM creation and shared by that VM's contexts.
use std::any::{Any, TypeId};
use anyhow::{bail, Result};
use rustc_hash::FxHashMap;

/// Per-VM extension registrations, frozen and injected by the host assembly.
///
/// Each Rust type may be registered once. Use distinct newtypes for distinct services.
/// The VM does not scan this storage for managed values; their roots must be managed
/// explicitly while the VM is alive.
///
/// ```
/// use z42_runtime_core::vm::ext::VmExtensions;
/// let mut extensions = VmExtensions::new();
/// extensions.register(String::from("tenant"))?;
/// let state = extensions.freeze();
/// assert_eq!(state.get::<String>().map(String::as_str), Some("tenant"));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Default)]
pub struct VmExtensions {
    state: ExtensionState,
}

impl VmExtensions {
    pub fn new() -> Self { Self::default() }

    /// Register an owned state. Duplicate types fail without replacing the original.
    pub fn register<T: Any + Send + Sync>(&mut self, value: T) -> Result<()> {
        let key = TypeId::of::<T>();
        if self.state.indices.contains_key(&key) {
            bail!("VM extension `{}` is already registered", std::any::type_name::<T>());
        }
        let index = self.state.values.len();
        self.state.values.push(Box::new(value));
        self.state.indices.insert(key, index);
        Ok(())
    }

    /// Default provider types are private to their owner and must not collide with callers.
    /// Providers are prefixed so user state is released before the services it can use.
    pub fn with_defaults(mut self, mut defaults: Self) -> Self {
        assert!(self.state.indices.keys().all(|key| !defaults.state.indices.contains_key(key)),
            "default VM extension type was registered by a caller");
        let offset = defaults.state.values.len();
        for (key, index) in std::mem::take(&mut self.state.indices) {
            defaults.state.indices.insert(key, index + offset);
        }
        defaults.state.values.append(&mut self.state.values);
        defaults
    }

    pub fn freeze(self) -> ExtensionState { self.state }
}

/// Only the pre-creation registrations can modify the index or move state owners.
/// All contexts borrow this same immutable storage through their shared core.
#[derive(Default)]
pub struct ExtensionState {
    indices: FxHashMap<TypeId, usize>,
    values: Vec<Box<dyn Any + Send + Sync>>,
}

impl ExtensionState {
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.indices.get(&TypeId::of::<T>())
            .and_then(|&index| self.values[index].downcast_ref())
    }
}

impl Drop for ExtensionState {
    fn drop(&mut self) {
        // Registration order is explicit; hash iteration must not decide resource lifetime.
        while let Some(value) = self.values.pop() { drop(value); }
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "ext_tests.rs"]
mod ext_tests;

#[cfg(all(test, z42_test_facade))]
use crate::assembly::context::VmContextFactory as _;
