//! Per-VM admission policy for native code; independent of component contracts.
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeOrigin { StaticLink, SharedLink, RuntimeLoad }

/// Native deployment settings captured before VM assembly. Denied code may already
/// be mapped by the system linker; denial prevents activation, not OS mapping.
#[derive(Clone, Debug)]
pub struct NativeLoading {
    pub search_dirs: Vec<PathBuf>,
    pub allow_static: bool,
    pub allow_shared: bool,
    pub allow_runtime: bool,
}
impl Default for NativeLoading {
    fn default() -> Self {
        let config = crate::config::runtime_config_snapshot();
        Self { search_dirs: super::native_paths::directories(), allow_static: !config.no_native_static,
            allow_shared: !config.no_native_shared, allow_runtime: !config.no_native_load }
    }
}
impl NativeLoading {
    pub(crate) fn allows(&self, origin: NativeOrigin) -> bool {
        match origin { NativeOrigin::StaticLink => self.allow_static,
            NativeOrigin::SharedLink => self.allow_shared, NativeOrigin::RuntimeLoad => self.allow_runtime }
    }
    pub(crate) fn check(&self, origin: NativeOrigin) -> anyhow::Result<()> {
        anyhow::ensure!(self.allows(origin), "native source {origin:?} is disabled by this VM's loading policy");
        Ok(())
    }
}
