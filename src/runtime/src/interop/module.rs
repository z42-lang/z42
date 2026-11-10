//! Validation and per-VM state for crate-owned native modules.
use anyhow::{Result, anyhow, bail};
use std::{collections::HashMap, ffi::c_void, sync::Arc};
use z42_abi::module as abi;

pub struct NativeModule {
    pub name: &'static str,
    pub(crate) functions: HashMap<&'static str, &'static abi::Function>,
    descriptor: &'static abi::Module,
    pub(crate) deferred: bool,
}

impl NativeModule {
    /// Validate without creating state or publishing any export.
    ///
    /// # Safety
    /// The entry and descriptor spans must be readable, immutable, and retained for
    /// the process lifetime. Callback state must support concurrent calls in one VM.
    pub unsafe fn parse(pointer: *const abi::Module) -> Result<Self> {
        unsafe { Self::parse_descriptor(pointer, false) }
    }

    /// Register immutable signatures without loading their implementation.
    ///
    /// # Safety
    /// The descriptor and spans must remain immutable for the process lifetime.
    pub unsafe fn declaration(pointer: *const abi::Module) -> Result<Self> {
        unsafe { Self::parse_descriptor(pointer, true) }
    }

    unsafe fn parse_descriptor(pointer: *const abi::Module, deferred: bool) -> Result<Self> {
        if pointer.is_null() || pointer as usize % std::mem::align_of::<abi::Module>() != 0 {
            bail!("invalid native module descriptor pointer");
        }
        let version = unsafe { pointer.cast::<u32>().read() };
        let size = unsafe { pointer.cast::<u32>().add(1).read() };
        if version != abi::MODULE_ABI_VERSION || size < std::mem::size_of::<abi::Module>() as u32 {
            bail!("native module ABI mismatch: version {version}, size {size}");
        }
        let descriptor = unsafe { &*pointer };
        if descriptor.create_state.is_some() != descriptor.destroy_state.is_some() {
            bail!("native module state creation and destruction must be paired");
        }
        if deferred && descriptor.create_state.is_some() {
            bail!("native declarations cannot create implementation state");
        }
        let name = unsafe { text(descriptor.name)? };
        if name.is_empty() {
            bail!("native module name must not be empty");
        }
        let mut functions = HashMap::new();
        for function in unsafe { span(descriptor.functions, descriptor.function_count)? } {
            let function_name = unsafe { text(function.name)? };
            if function_name.is_empty() {
                bail!("empty native export in `{name}`");
            }
            if functions.insert(function_name, function).is_some() {
                bail!("duplicate native export `{name}::{function_name}`");
            }
            if !deferred && function.invoke.is_none() {
                bail!("missing invoke entry for `{name}::{function_name}`");
            }
            if deferred && function.invoke.is_some() {
                bail!("native declaration `{name}::{function_name}` must not contain a wrapper");
            }
            for kind in unsafe { span(function.parameters.data, function.parameters.len)? } {
                if !matches!(
                    *kind,
                    abi::I64
                        | abi::F64
                        | abi::BOOL
                        | abi::STR
                        | abi::OPT_STR
                        | abi::BYTES
                        | abi::STR_ARRAY
                ) {
                    bail!("unsupported native parameter kind {kind} in `{name}::{function_name}`");
                }
            }
            if !matches!(
                function.result,
                abi::I64
                    | abi::F64
                    | abi::BOOL
                    | abi::STR
                    | abi::OPT_STR
                    | abi::BYTES
                    | abi::STR_ARRAY
                    | abi::VOID
            ) {
                bail!("unsupported native result kind in `{name}::{function_name}`");
            }
            if function.flags & !(abi::PURE | abi::BLOCKING | abi::REENTERS) != 0 {
                bail!("unknown native flags in `{name}::{function_name}`");
            }
        }
        Ok(Self {
            name,
            functions,
            descriptor,
            deferred,
        })
    }

    pub fn function(&self, name: &str) -> Option<&'static abi::Function> {
        self.functions.get(name).copied()
    }
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }
    pub(crate) fn has_state(&self) -> bool {
        self.descriptor.create_state.is_some()
    }
    pub fn functions(&self) -> impl Iterator<Item = (&'static str, &'static abi::Function)> + '_ {
        self.functions
            .iter()
            .map(|(&name, &function)| (name, function))
    }

    fn same_interface(&self, other: &Self) -> bool {
        self.functions.len() == other.functions.len()
            && self.functions.iter().all(|(name, function)| {
                let Some(other) = other.functions.get(name) else {
                    return false;
                };
                function.result == other.result
                    && function.flags == other.flags
                    && unsafe { span(function.parameters.data, function.parameters.len).ok() }
                        == unsafe { span(other.parameters.data, other.parameters.len).ok() }
            })
            && (self.deferred
                || other.deferred
                || self.descriptor.create_state.is_some()
                    == other.descriptor.create_state.is_some())
    }
}

/// Register a retained native image in one VM. Repeated declarations of the same
/// module interface are idempotent; a conflicting interface never replaces the original.
///
/// # Safety
/// The descriptor must satisfy NativeModule::parse, including process-lifetime code/data.
pub unsafe fn register_native_module(
    ctx: &crate::context::VmContext,
    descriptor: *const abi::Module,
) -> Result<()> {
    let module = unsafe { NativeModule::parse(descriptor)? };
    let state = super::state::state(ctx)?;
    if state.providers.provider_functions(module.name).is_some() {
        bail!(
            "native module `{}` conflicts with a builtin provider",
            module.name
        );
    }
    for (name, _) in module.functions() {
        if state
            .providers
            .functions()
            .iter()
            .any(|function| function.name == name)
        {
            bail!("native export `{name}` conflicts with an existing builtin");
        }
    }
    let mut modules = state.external_modules.write();
    if let Some(existing) = modules
        .get(module.name)
        .cloned()
        .or_else(|| state.providers.module(module.name))
    {
        if !existing.same_interface(&module) {
            bail!("conflicting native module interface `{}`", module.name);
        }
        if !existing.deferred {
            return Ok(());
        }
    }
    modules.insert(module.name, Arc::new(module));
    Ok(())
}

/// Resolve a declaration through the host's linked/dynamic library policy. Only a
/// fully validated implementation is published; failures leave the declaration intact.
pub(crate) fn materialize(
    ctx: &crate::context::VmContext,
    module: Arc<NativeModule>,
) -> Result<Arc<NativeModule>> {
    if !module.deferred {
        return Ok(module);
    }
    let entry_name = format!("{}_module_v1", module.name);
    let pointer = crate::loader::native::resolve_symbol(ctx, module.name, &entry_name)?;
    let entry: abi::ModuleEntry = unsafe { std::mem::transmute(pointer) };
    let descriptor = unsafe { entry() };
    let implementation = unsafe { NativeModule::parse(descriptor)? };
    if module.name != implementation.name || !module.same_interface(&implementation) {
        bail!(
            "native implementation does not match declaration `{}`",
            module.name
        );
    }
    unsafe { register_native_module(ctx, descriptor)? };
    let resolved = resolve_native_module(ctx, module.name)?
        .ok_or_else(|| anyhow!("native implementation was not registered"))?;
    if resolved.deferred {
        bail!("native image did not implement module `{}`", module.name);
    }
    Ok(resolved)
}

pub fn resolve_native_module(
    ctx: &crate::context::VmContext,
    name: &str,
) -> Result<Option<Arc<NativeModule>>> {
    let state = super::state::state(ctx)?;
    Ok(state
        .external_modules
        .read()
        .get(name)
        .cloned()
        .or_else(|| state.providers.module(name)))
}

pub(crate) struct ModuleInstance {
    pub module: Arc<NativeModule>,
    pub state: *mut c_void,
}
// Registration requires provider-owned state to support concurrent VM calls.
unsafe impl Send for ModuleInstance {}
unsafe impl Sync for ModuleInstance {}
impl ModuleInstance {
    pub fn create(module: Arc<NativeModule>) -> Result<Self> {
        let state = match module.descriptor.create_state {
            Some(create) => {
                let state = unsafe { create() };
                if state.is_null() {
                    bail!("native module `{}` failed to create VM state", module.name);
                }
                state
            }
            None => std::ptr::null_mut(),
        };
        Ok(Self { module, state })
    }
}
impl Drop for ModuleInstance {
    fn drop(&mut self) {
        if let Some(destroy) = self.module.descriptor.destroy_state {
            unsafe { destroy(self.state) };
        }
    }
}

pub(crate) unsafe fn span<'a, T>(data: *const T, len: usize) -> Result<&'a [T]> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null()
        || data as usize % std::mem::align_of::<T>() != 0
        || len > isize::MAX as usize / std::mem::size_of::<T>()
    {
        bail!("invalid native descriptor span");
    }
    Ok(unsafe { std::slice::from_raw_parts(data, len) })
}
pub(crate) unsafe fn text<'a>(value: abi::Text) -> Result<&'a str> {
    std::str::from_utf8(unsafe { span(value.data, value.len)? }).map_err(|error| anyhow!(error))
}
