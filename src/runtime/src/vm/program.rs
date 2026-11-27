//! Program lifetime, loading and first-call preparation.
use super::{
    assembly::{self, ComponentInputs},
    VmContext,
};
use crate::{
    exec::EntryError,
    loader::{self, program::ZpkgResolver, ImageCache, NativeLoading},
    metadata::Module,
    object::Value,
    pal::fs::Vfs,
    zpkg::ExecMode,
};
use anyhow::{anyhow, bail, Result};
use std::{
    path::PathBuf,
    pin::Pin,
    sync::{Arc, OnceLock},
};

enum ProgramSource<'a> {
    File(&'a str),
    Bytes(&'a [u8]),
}

/// Host-provided inputs. A byte source has no implicit working-directory search.
pub struct LoadOptions {
    /// Allowed native deployment sources and search directories, captured per VM.
    pub native: NativeLoading,
    /// Permit optional sampling and heap inspection; false never activates a profiler.
    pub allow_profiling: bool,
    pub mode: ExecMode,
    pub vfs: Arc<dyn Vfs>,
    pub search_dirs: Vec<PathBuf>,
    pub resolver: Option<Arc<dyn ZpkgResolver>>,
    /// Preload declared imports for hosts that resolve dependency entries before invocation.
    pub eager_dependencies: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            native: Default::default(),
            allow_profiling: true,
            mode: default_mode(),
            vfs: crate::pal::fs::default_vfs(),
            search_dirs: Vec::new(),
            resolver: None,
            eager_dependencies: false,
        }
    }
}

/// Build-default execution mode: JIT when compiled in (make-jit-default),
/// else Interp (jit-less builds — wasm / `--features interp-only`). Callers
/// without an explicit mode (embedding one-shot) use this.
pub fn default_mode() -> ExecMode {
    #[cfg(all(feature = "jit", z42_jit_supported))]
    {
        ExecMode::Jit
    }
    #[cfg(not(all(feature = "jit", z42_jit_supported)))]
    {
        ExecMode::Interp
    }
}

fn load_program(
    source: ProgramSource<'_>,
    request: LoadOptions,
    image_cache: Option<&ImageCache>,
) -> Result<LoadedProgram> {
    let components = ComponentInputs {
        profiler: if request.allow_profiling {
            Default::default()
        } else {
            crate::components::api::BackendSource::Disabled
        },
        ..Default::default()
    };
    load_program_with_components(source, request, image_cache, &components)
}

fn load_program_with_components(
    source: ProgramSource<'_>,
    request: LoadOptions,
    image_cache: Option<&ImageCache>,
    components: &ComponentInputs,
) -> Result<LoadedProgram> {
    let (artifact, path) = match source {
        ProgramSource::File(path) => (loader::load_artifact(&*request.vfs, path)?, Some(path)),
        ProgramSource::Bytes(bytes) => (
            loader::load_artifact_from_bytes_cached(bytes, image_cache)?,
            None,
        ),
    };
    let prepared = loader::prepare_startup(
        artifact,
        path,
        &*request.vfs,
        request.search_dirs,
        request.resolver.as_deref(),
        request.eager_dependencies || matches!(request.mode, ExecMode::Aot),
        image_cache,
    )?;
    let jit_config = crate::config::runtime_config_snapshot().jit_config();
    let ctx = assembly::build(
        prepared.module,
        assembly::ProgramInputs {
            vfs: request.vfs,
            packages: prepared.packages,
        },
        assembly::AssemblyOptions {
            native: request.native,
            mode: request.mode,
            jit_config,
            components,
            freeze_extensions: crate::composition::freeze,
        },
        prepared.initial_events,
    )?;
    LoadedProgram::new(ctx, request.mode, prepared.entry_hint)
}

/// Query whether this host can prepare an execution mode, without loading code
/// or constructing a VM. Component identities remain an internal concern.
pub fn execution_available(mode: ExecMode) -> bool {
    use crate::components::api::{self, ExecutionRole};
    match mode {
        ExecMode::Interp => api::execution_available(ExecutionRole::Baseline),
        ExecMode::Jit => api::execution_available(ExecutionRole::Optimizing),
        ExecMode::Aot => false,
    }
}

/// One loaded module and its VM. Initialization is shared by all entry invocations.
/// Drive a context from one thread at a time; worker execution uses its own context.
pub struct LoadedProgram {
    ctx: Pin<Box<VmContext>>,
    module: Arc<Module>,
    mode: ExecMode,
    entry_hint: Option<String>,
    initialized: OnceLock<()>,
}

impl LoadedProgram {
    pub fn load_file(path: &str, options: LoadOptions) -> Result<Self> {
        load_program(ProgramSource::File(path), options, None)
    }

    pub fn load_bytes(bytes: &[u8], options: LoadOptions) -> Result<Self> {
        load_program(ProgramSource::Bytes(bytes), options, None)
    }

    /// Decode package definitions through the session's shared cache. Linkage,
    /// statics and initialization state remain per VM.
    pub fn load_bytes_with_cache(
        bytes: &[u8],
        options: LoadOptions,
        image_cache: &ImageCache,
    ) -> Result<Self> {
        load_program(ProgramSource::Bytes(bytes), options, Some(image_cache))
    }

    #[cfg(test)]
    pub(crate) fn load_bytes_with_components(
        bytes: &[u8],
        options: LoadOptions,
        components: ComponentInputs,
    ) -> Result<Self> {
        load_program_with_components(ProgramSource::Bytes(bytes), options, None, &components)
    }

    pub(crate) fn new(
        ctx: Pin<Box<VmContext>>,
        mode: ExecMode,
        entry_hint: Option<String>,
    ) -> Result<Self> {
        let module = ctx
            .module()
            .cloned()
            .ok_or_else(|| anyhow!("loaded program has no module"))?;
        Ok(Self {
            ctx,
            module,
            mode,
            entry_hint,
            initialized: OnceLock::new(),
        })
    }

    #[cfg(any(test, feature = "shared-core", feature = "jit-test-support"))]
    pub(crate) fn context(&self) -> &VmContext {
        &self.ctx
    }

    pub(crate) fn set_program_args(&self, args: Vec<String>) -> Result<()> {
        super::app_state::set_program_args(&self.ctx, args)
    }

    pub(crate) fn report_on_exit(&self) {
        crate::composition::report_on_exit(&self.ctx);
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn resolve_entry(&self, name: &str) -> Result<usize> {
        resolve_fqn(self.module(), name)
    }

    /// Invoke an entry. Managed arguments are rooted across preparation and execution;
    /// callers retaining a managed return value must root it before subsequent VM calls.
    pub fn invoke(&self, name: &str, args: &[Value]) -> Result<Option<Value>> {
        self.invoke_index(self.resolve_entry(name)?, args)
            .map_err(EntryError::into_diagnostic)
    }

    pub fn run_entry(&self, entry: Option<&str>) -> Result<()> {
        let entry = entry.or(self.entry_hint.as_deref()).ok_or_else(|| {
            anyhow!("no entry point: pass the function name or rebuild with an entry in the zpkg")
        })?;
        self.invoke(entry, &[]).map(|_| ())
    }

    pub(crate) fn invoke_index(
        &self,
        index: usize,
        args: &[Value],
    ) -> Result<Option<Value>, EntryError> {
        let ctx = &self.ctx;
        crate::exec::invoke_entry(ctx, self.module(), self.mode, index, args, || {
            self.initialized.get_or_init(|| {
                ctx.statics().clear_for_rerun();
                ctx.loader().drain_pending_type_loads();
            });
        })
    }
}

/// Look up a fully-qualified function name in the program module's function
/// table. Accepts both the dot form `"namespace.Type.method"` and the
/// `::` form `"namespace.Type::method"` (the spec uses the latter for
/// readability; the bytecode currently records the former).
fn resolve_fqn(module: &Module, fqn: &str) -> Result<usize> {
    if fqn.is_empty() {
        bail!("fqn is empty");
    }
    if let Some(&idx) = module.func_index.get(fqn) {
        return Ok(idx);
    }
    // Tolerate the "ns.Type::method" form by normalising `::` → `.`.
    if fqn.contains("::") {
        let normalised = fqn.replace("::", ".");
        if let Some(&idx) = module.func_index.get(&normalised) {
            return Ok(idx);
        }
    }
    Err(anyhow!(
        "function `{fqn}` not found in module `{}`",
        module.name
    ))
}
