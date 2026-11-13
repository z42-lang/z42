//! Loaded programs shared by command-line and embedded hosts.
use crate::{
    zpkg::ExecMode,
    object::Value,
    pal::fs::Vfs,
    context::VmContext, exec::call::outcome::ExecOutcome, metadata::Module,
};
use anyhow::{anyhow, bail, Result};
use std::{
    path::PathBuf,
    pin::Pin,
    sync::{Arc, OnceLock},
};

/// Host-provided inputs. A byte source has no implicit working-directory search.
pub struct LoadOptions {
    /// Native component selection, attached once before this program executes.
    pub components: super::ComponentOptions,
    pub backends: super::BackendOptions,
    pub mode: ExecMode,
    pub vfs: Arc<dyn Vfs>,
    pub search_dirs: Vec<PathBuf>,
    pub resolver: Option<Arc<dyn super::ZpkgResolver>>,
    /// Preload declared imports for hosts that resolve dependency entries before invocation.
    pub eager_dependencies: bool,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            components: Default::default(),
            backends: Default::default(),
            mode: super::default_mode(),
            vfs: crate::pal::fs::default_vfs(),
            search_dirs: Vec::new(),
            resolver: None,
            eager_dependencies: false,
        }
    }
}

/// One loaded module and its VM. Initialization is shared by all entry invocations.
/// Drive a context from one thread at a time; worker execution uses its own context.
pub struct LoadedProgram {
    pub(crate) ctx: Pin<Box<VmContext>>,
    module: Arc<Module>,
    mode: ExecMode,
    entry_hint: Option<String>,
    initialized: OnceLock<Result<(), String>>,
}

impl LoadedProgram {
    pub(super) fn new(
        ctx: Pin<Box<VmContext>>,
        mode: ExecMode,
        entry_hint: Option<String>,
    ) -> Result<Self> {
        let module = ctx
            .core()
            .runtime()
            .module
            .clone()
            .ok_or_else(|| anyhow!("loaded program has no module"))?;
        Ok(Self {
            ctx,
            module,
            mode,
            entry_hint,
            initialized: OnceLock::new(),
        })
    }

    pub fn load_file(path: &str, options: LoadOptions) -> Result<Self> {
        let artifact = crate::host::artifact::load_artifact(&*options.vfs, path)?;
        super::load::prepare(artifact, Some(path), &options)
    }

    pub fn load_bytes(bytes: &[u8], options: LoadOptions) -> Result<Self> {
        let artifact = crate::host::artifact::load_artifact_from_bytes(bytes)?;
        super::load::prepare(artifact, None, &options)
    }

    pub fn context(&self) -> &VmContext {
        &self.ctx
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
    }

    pub fn run_entry(&self, entry: Option<&str>) -> Result<()> {
        let entry = entry.or(self.entry_hint.as_deref()).ok_or_else(|| {
            anyhow!("no entry point: pass the function name or rebuild with an entry in the zpkg")
        })?;
        self.invoke(entry, &[]).map(|_| ())
    }

    pub(crate) fn invoke_index(&self, index: usize, args: &[Value]) -> Result<Option<Value>> {
        let func = self
            .module
            .functions
            .get(index)
            .ok_or_else(|| anyhow!("entry function index out of bounds"))?;
        if args.len() != func.param_count {
            bail!(
                "arg-count-mismatch: function `{}` expects {} arg(s), got {}",
                func.name,
                func.param_count,
                args.len()
            );
        }
        let ctx = self.context();
        #[cfg(feature = "native-interop")]
        let _vm_guard = crate::exec::active_vm::VmGuard::enter(ctx);
        let _heap_guard = crate::exec::ambient::HeapGuard::enter(ctx.heap());
        struct Roots<'a>(&'a crate::gc::Heap, Vec<crate::gc::RootHandle>);
        impl Drop for Roots<'_> {
            fn drop(&mut self) {
                for root in self.1.drain(..) {
                    self.0.unpin_root(root);
                }
            }
        }
        let _roots = Roots(
            ctx.heap(),
            args.iter().map(|v| ctx.heap().pin_root(*v)).collect(),
        );
        let initialized = self.initialized.get_or_init(|| {
            ctx.static_fields_clear();
            ctx.drain_pending_type_loads();
            match ctx.take_static_init_error() {
                Some(e) => Err(e),
                None => Ok(()),
            }
        });
        if let Err(error) = initialized {
            bail!("uncaught exception during static initialization: {error}");
        }
        let outcome = ctx.core().exec().engines.invoke_mode(self.mode, ctx, self.module(), func, args);
        if crate::exec::stack_guard::is_fatal(ctx) {
            bail!(
                "{}",
                crate::exec::stack_guard::report().unwrap_or_else(|| format!(
                    "{}stack overflow",
                    crate::exec::stack_guard::FATAL_PREFIX
                ))
            );
        }
        let outcome = outcome?;
        if let Some(error) = ctx.take_static_init_error() {
            bail!("uncaught exception during static initialization: {error}");
        }
        match outcome {
            ExecOutcome::Returned(value) => Ok(value),
            ExecOutcome::Thrown(value) => bail!(
                "{}",
                crate::exec::exception::format_uncaught(&value, ctx, self.module())
            ),
        }
    }
}

/// Look up a fully-qualified function name in a host-module's function
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
