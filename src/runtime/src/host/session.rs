//! Shared transactions for the process-wide embedding session.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use super::abi_config::Z42ExecMode;
use super::error::Z42HostStatus as Status;
use super::{LoadedProgram, ZpkgResolver};
use crate::corelib::io::{self, HostSinks, OutputSink};
use crate::exec::EntryError;
use crate::loader::ImageCache;
use crate::object::Value;
use crate::pal::fs::Vfs;

type Token = usize;
type Result<T> = std::result::Result<T, Failure>;

pub(super) struct Failure {
    pub(super) status: Status,
    pub(super) message: String,
}

impl Failure {
    pub(super) fn new(status: Status, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub(super) fn not_initialized() -> Self {
        Self::new(
            Status::NotInit,
            "VM is not initialized or the session ended during the call",
        )
    }
}

/// Owned configuration for a host session.
pub(super) struct SessionConfig {
    pub(super) exec_mode: Z42ExecMode,
    pub(super) search_paths: Vec<String>,
    pub(super) zpkg_resolver: Option<Arc<dyn ZpkgResolver>>,
    pub(super) file_resolver: Option<Arc<dyn super::FileResolver>>,
    pub(super) vfs: Arc<dyn Vfs>,
}

impl SessionConfig {
    pub(super) fn load_options(&self) -> crate::host::LoadOptions {
        crate::host::LoadOptions {
            native: Default::default(),
            allow_profiling: true,
            mode: match self.exec_mode {
                Z42ExecMode::Default => crate::host::default_mode(),
                Z42ExecMode::Interp => crate::zpkg::ExecMode::Interp,
                Z42ExecMode::Jit => crate::zpkg::ExecMode::Jit,
                Z42ExecMode::Aot => crate::zpkg::ExecMode::Aot,
            },
            vfs: Arc::clone(&self.vfs),
            search_dirs: self.search_paths.iter().map(PathBuf::from).collect(),
            resolver: self.zpkg_resolver.clone(),
            file_resolver: self.file_resolver.clone(),
            eager_dependencies: true,
        }
    }
}

impl std::fmt::Debug for SessionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionConfig")
            .field("exec_mode", &self.exec_mode)
            .field("search_paths", &self.search_paths)
            .field("zpkg_resolver", &self.zpkg_resolver.is_some())
            .finish()
    }
}

#[derive(Clone, Copy)]
struct Entry {
    module: Token,
    function: usize,
}

struct Session {
    token: Token,
    config: SessionConfig,
    image_cache: Arc<ImageCache>,
    modules: HashMap<Token, Arc<LoadedProgram>>,
    entries: HashMap<Token, Entry>,
}

struct Registry {
    next: Token,
    session: Option<Session>,
}

impl Registry {
    // Tokens cross the ABI as opaque non-null pointers and are never dereferenced.
    // Issuing them across sessions prevents a stale token from aliasing a new one.
    fn allocate(&mut self) -> Result<Token> {
        let token = self.next;
        self.next = token
            .checked_add(1)
            .ok_or_else(|| Failure::new(Status::Internal, "host handles exhausted"))?;
        Ok(token)
    }

    fn session(&self, token: Token) -> Result<&Session> {
        self.session
            .as_ref()
            .filter(|session| session.token == token)
            .ok_or_else(Failure::not_initialized)
    }

    fn session_mut(&mut self, token: Token) -> Result<&mut Session> {
        self.session
            .as_mut()
            .filter(|session| session.token == token)
            .ok_or_else(Failure::not_initialized)
    }
}

static HOST: RwLock<Registry> = RwLock::new(Registry {
    next: 1,
    session: None,
});

fn read() -> Result<RwLockReadGuard<'static, Registry>> {
    HOST.read()
        .map_err(|_| Failure::new(Status::Internal, "host state lock poisoned"))
}

fn write() -> Result<RwLockWriteGuard<'static, Registry>> {
    HOST.write()
        .map_err(|_| Failure::new(Status::Internal, "host state lock poisoned"))
}

pub(super) fn current() -> Result<Token> {
    read()?
        .session
        .as_ref()
        .map(|session| session.token)
        .ok_or_else(Failure::not_initialized)
}

fn probe_prelude(vfs: &dyn Vfs, search_paths: &[String]) -> Result<()> {
    crate::loader::probe_prelude(vfs, search_paths)
        .map_err(|error| Failure::new(Status::BadConfig, format!("corelib probe failed: {error:#}")))
}

pub(super) fn initialize(config: SessionConfig, sinks: HostSinks) -> Result<Token> {
    #[cfg(feature = "native-interop")]
    crate::interop::exports::install_panic_hook();

    if config.file_resolver.is_none() {
        probe_prelude(&*config.vfs, &config.search_paths)?;
    }

    let mut registry = write()?;
    if registry.session.is_some() {
        return Err(Failure::new(
            Status::AlreadyInit,
            "VM is already initialized",
        ));
    }
    let token = registry.allocate()?;
    let previous_sinks = io::replace_host_sinks(sinks);
    registry.session = Some(Session {
        token,
        config,
        image_cache: Arc::new(ImageCache::default()),
        modules: HashMap::new(),
        entries: HashMap::new(),
    });
    drop(registry);
    drop(previous_sinks);
    Ok(token)
}

pub(super) fn shutdown(token: Token) -> Result<()> {
    let mut registry = write()?;
    registry.session(token)?;
    let sinks = io::replace_host_sinks(HostSinks::default());
    let retired = registry.session.take();
    drop(registry);
    // Resource and Rust callback destructors can re-enter the host.
    drop(sinks);
    drop(retired);
    Ok(())
}

pub(super) fn load(token: Token, name: Option<&str>, bytes: &[u8]) -> Result<Token> {
    let (options, cache) = {
        let registry = read()?;
        let session = registry.session(token)?;
        (
            session.config.load_options(),
            Arc::clone(&session.image_cache),
        )
    };
    let program = Arc::new(
        LoadedProgram::load_named_bytes_with_cache(bytes, name, options, &cache)
            .map_err(|error| Failure::new(Status::BadZbc, format!("{error:#}")))?,
    );

    let mut registry = write()?;
    registry.session(token)?;
    let module = registry.allocate()?;
    registry.session_mut(token)?.modules.insert(module, program);
    Ok(module)
}

pub(super) fn resolve(token: Token, module: Token, fqn: &str) -> Result<Token> {
    let mut registry = write()?;
    let session = registry.session(token)?;
    if module < session.token {
        return Err(Failure::not_initialized());
    }
    let program = session
        .modules
        .get(&module)
        .ok_or_else(|| Failure::new(Status::EntryNotFound, "module handle is stale or invalid"))?;
    let function = program
        .resolve_entry(fqn)
        .map_err(|error| Failure::new(Status::EntryNotFound, format!("{error:#}")))?;
    let entry = registry.allocate()?;
    registry
        .session_mut(token)?
        .entries
        .insert(entry, Entry { module, function });
    Ok(entry)
}

pub(super) fn invoke(
    token: Token,
    entry: Token,
    arguments: impl FnOnce() -> Result<Vec<Value>>,
) -> Result<Option<Value>> {
    let (program, function) = {
        let registry = read()?;
        let session = registry.session(token)?;
        if entry < session.token {
            return Err(Failure::not_initialized());
        }
        let entry = session.entries.get(&entry).ok_or_else(|| {
            Failure::new(Status::EntryNotFound, "entry handle is stale or invalid")
        })?;
        let program = session.modules.get(&entry.module).ok_or_else(|| {
            Failure::new(Status::Internal, "module backing this entry is missing")
        })?;
        (Arc::clone(program), entry.function)
    };
    let args = arguments()?;
    let _sink = io::HostSinkGuard::enter();
    program.invoke_index(function, &args).map_err(|error| {
        let status = match &error {
            EntryError::ArgMismatch(_) => Status::ArgMismatch,
            EntryError::Uncaught(_) => Status::VmException,
            EntryError::Fatal(_) => {
                let _ = crate::exec::stack_guard::take_report();
                Status::Fatal
            }
            EntryError::Internal(_) => Status::Internal,
        };
        Failure::new(status, format!("{:#}", error.into_diagnostic()))
    })
}

pub(super) fn set_sink(token: Token, stderr: bool, sink: Option<OutputSink>) -> Result<()> {
    let registry = read()?;
    registry.session(token)?;
    let previous = io::replace_host_sink(stderr, sink);
    drop(registry);
    drop(previous);
    Ok(())
}

pub(super) fn set_resolver(token: Token, resolver: Arc<dyn ZpkgResolver>) -> Result<()> {
    let retired = write()?
        .session_mut(token)?
        .config
        .zpkg_resolver
        .replace(resolver);
    drop(retired);
    Ok(())
}

pub(super) fn set_vfs(token: Token, vfs: Arc<dyn Vfs>) -> Result<()> {
    let paths_to_probe = {
        let registry = read()?;
        let config = &registry.session(token)?.config;
        config.file_resolver.is_none().then(|| config.search_paths.clone())
    };
    if let Some(paths) = paths_to_probe {
        probe_prelude(&*vfs, &paths)?;
    }
    let retired = std::mem::replace(&mut write()?.session_mut(token)?.config.vfs, vfs);
    drop(retired);
    Ok(())
}

#[cfg(feature = "jit-test-support")]
pub(crate) fn first_program() -> Option<Arc<LoadedProgram>> {
    read()
        .ok()?
        .session
        .as_ref()?
        .modules
        .iter()
        .min_by_key(|(token, _)| *token)
        .map(|(_, program)| Arc::clone(program))
}
