//! Rust embedding requests and the callable program handle.
use crate::{
    metadata::Module,
    object::Value,
    pal::fs::Vfs,
    vm::{
        program::{self, Program, ProgramRequest, ProgramSource},
        VmContext,
    },
    zpkg::ExecMode,
};
use anyhow::Result;
use std::{path::PathBuf, sync::Arc};

/// Host-provided inputs. A byte source has no implicit working-directory search.
pub struct LoadOptions {
    /// Allowed native deployment sources and search directories, captured per VM.
    pub native: crate::loader::NativeLoading,
    /// Permit optional sampling and heap inspection; false never activates a profiler.
    pub allow_profiling: bool,
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
            native: Default::default(),
            allow_profiling: true,
            mode: super::default_mode(),
            vfs: crate::pal::fs::default_vfs(),
            search_dirs: Vec::new(),
            resolver: None,
            eager_dependencies: false,
        }
    }
}

impl From<LoadOptions> for ProgramRequest {
    fn from(options: LoadOptions) -> Self {
        Self {
            native: options.native,
            allow_profiling: options.allow_profiling,
            mode: options.mode,
            vfs: options.vfs,
            search_dirs: options.search_dirs,
            resolver: options.resolver,
            eager_dependencies: options.eager_dependencies,
        }
    }
}

/// One loaded module and its VM. Initialization is shared by all entry invocations.
/// Drive a context from one thread at a time; worker execution uses its own context.
pub struct LoadedProgram(Program);

impl LoadedProgram {
    #[cfg(any(test, feature = "jit-test-support"))]
    pub(crate) fn new(
        ctx: std::pin::Pin<Box<VmContext>>,
        mode: ExecMode,
        entry_hint: Option<String>,
    ) -> Result<Self> {
        Program::new(ctx, mode, entry_hint).map(Self)
    }

    pub fn load_file(path: &str, options: LoadOptions) -> Result<Self> {
        program::load_program(ProgramSource::File(path), options.into(), None).map(Self)
    }

    pub fn load_bytes(bytes: &[u8], options: LoadOptions) -> Result<Self> {
        program::load_program(ProgramSource::Bytes(bytes), options.into(), None).map(Self)
    }

    /// Decode package definitions through the session's shared cache. Linkage,
    /// statics and initialization state remain per VM.
    pub fn load_bytes_with_cache(
        bytes: &[u8],
        options: LoadOptions,
        image_cache: &crate::loader::ImageCache,
    ) -> Result<Self> {
        program::load_program(
            ProgramSource::Bytes(bytes),
            options.into(),
            Some(image_cache),
        )
        .map(Self)
    }

    #[cfg(test)]
    pub(crate) fn load_bytes_with_components(
        bytes: &[u8],
        options: LoadOptions,
        components: crate::vm::assembly::ComponentInputs,
    ) -> Result<Self> {
        program::load_program_with_components(
            ProgramSource::Bytes(bytes),
            options.into(),
            None,
            &components,
        )
        .map(Self)
    }

    /// Low-level channel for the packaged engine verification and JIT harnesses.
    #[doc(hidden)]
    pub fn context(&self) -> &VmContext {
        self.0.context()
    }

    pub fn module(&self) -> &Module {
        self.0.module()
    }

    pub fn resolve_entry(&self, name: &str) -> Result<usize> {
        self.0.resolve_entry(name)
    }

    /// Invoke an entry. Managed arguments are rooted across preparation and execution;
    /// callers retaining a managed return value must root it before subsequent VM calls.
    pub fn invoke(&self, name: &str, args: &[Value]) -> Result<Option<Value>> {
        self.0.invoke(name, args)
    }

    pub fn run_entry(&self, entry: Option<&str>) -> Result<()> {
        self.0.run_entry(entry)
    }

    pub(crate) fn invoke_index(&self, index: usize, args: &[Value]) -> Result<Option<Value>> {
        self.0.invoke_index(index, args)
    }
}
