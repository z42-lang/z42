//! Program lifetime, loading and first-call preparation.
use super::{
    assembly::{self, ComponentInputs},
    VmContext,
};
use crate::{
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

pub(crate) enum ProgramSource<'a> {
    File(&'a str),
    Bytes(&'a [u8]),
}

pub(crate) struct ProgramRequest {
    pub(crate) native: NativeLoading,
    pub(crate) allow_profiling: bool,
    pub(crate) mode: ExecMode,
    pub(crate) vfs: Arc<dyn Vfs>,
    pub(crate) search_dirs: Vec<PathBuf>,
    pub(crate) resolver: Option<Arc<dyn ZpkgResolver>>,
    pub(crate) eager_dependencies: bool,
}

pub(crate) fn load_program(
    source: ProgramSource<'_>,
    request: ProgramRequest,
    image_cache: Option<&ImageCache>,
) -> Result<Program> {
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

pub(crate) fn load_program_with_components(
    source: ProgramSource<'_>,
    request: ProgramRequest,
    image_cache: Option<&ImageCache>,
    components: &ComponentInputs,
) -> Result<Program> {
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
    Program::new(ctx, request.mode, prepared.entry_hint)
}

pub(crate) fn execution_available(mode: ExecMode) -> bool {
    use crate::components::api::{self, ExecutionRole};
    match mode {
        ExecMode::Interp => api::execution_available(ExecutionRole::Baseline),
        ExecMode::Jit => api::execution_available(ExecutionRole::Optimizing),
        ExecMode::Aot => false,
    }
}

/// One loaded module and its VM. Initialization is shared by all entry invocations.
/// Drive a context from one thread at a time; worker execution uses its own context.
pub(crate) struct Program {
    ctx: Pin<Box<VmContext>>,
    module: Arc<Module>,
    mode: ExecMode,
    entry_hint: Option<String>,
    initialized: OnceLock<()>,
}

impl Program {
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

    pub(crate) fn context(&self) -> &VmContext {
        &self.ctx
    }
    pub(crate) fn module(&self) -> &Module {
        &self.module
    }

    pub(crate) fn resolve_entry(&self, name: &str) -> Result<usize> {
        resolve_fqn(self.module(), name)
    }

    pub(crate) fn invoke(&self, name: &str, args: &[Value]) -> Result<Option<Value>> {
        self.invoke_index(self.resolve_entry(name)?, args)
    }

    pub(crate) fn run_entry(&self, entry: Option<&str>) -> Result<()> {
        let entry = entry.or(self.entry_hint.as_deref()).ok_or_else(|| {
            anyhow!("no entry point: pass the function name or rebuild with an entry in the zpkg")
        })?;
        self.invoke(entry, &[]).map(|_| ())
    }

    pub(crate) fn invoke_index(&self, index: usize, args: &[Value]) -> Result<Option<Value>> {
        let ctx = self.context();
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
