pub mod components;
pub mod context;
pub use context::{VmCore, VmContext};

pub mod diagnostics;
pub mod ext;
pub mod threads;
pub mod runtime;
pub mod loader;
pub mod exec;
pub mod config;

use crate::vm::runtime::Module;
use crate::zpkg::ExecMode;
use anyhow::{bail, Result};

/// Top-level VM: holds run-time configuration (exec mode) and dispatches to
/// the appropriate backend.
///
/// **add-threading-stdlib **: the user's compiled `Module` moved
/// from `Vm` into `VmCore` (accessed via `ctx.core.runtime.module`). This lets a
/// spawned thread share the same Module without a Vm clone. Vm is now a
/// thin run-config wrapper.
pub struct Vm {
    pub default_mode: ExecMode,
}

impl Vm {
    pub fn new(default_mode: ExecMode) -> Self {
        Vm { default_mode }
    }

    /// Execute the module's entry point.
    ///
    /// `ctx` carries the runtime-mutable state (static fields, lazy loader,
    /// pending exception) AND the user's compiled Module via `ctx.core.runtime.module`.
    /// Caller is responsible for constructing `VmContext` with
    /// [`VmContext::with_module`](crate::vm::context::VmContext::with_module),
    /// and for `ctx.install_lazy_loader` before calling `run` if dependencies
    /// need lazy-loading.
    ///
    /// Entry resolution (strict, since  auto-detect-main +
    /// drop-cli-entry-fallback): `hint` must be `Some(name)`. The compiler
    /// (`PackageCompiler.BuildTarget.AutoDetectEntry`) bakes `Entry` into
    /// every exe zpkg; the artifact loader passes it as `hint`. CLI users
    /// can override with the positional `[entry]` arg on `z42vm`. No
    /// silent fallback chain — missing entry → hard error.
    pub fn run(&self, ctx: &VmContext, hint: Option<&str>) -> Result<()> {
        let module = ctx.core.runtime.module.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Vm::run requires the VmContext to carry a Module — \
                 construct via `VmContext::with_module(module)` instead of \
                 `VmContext::new()`"
            )
        })?;
        let entry_name = Self::resolve_entry_in(module, hint)?;

        // FuncRef slots + dispatch tokens (shared with the embedding host path).
        crate::vm::runtime::resolve::resolve_module(module, ctx);

        let entry = module
            .functions
            .iter()
            .find(|f| f.name == entry_name)
            .ok_or_else(|| anyhow::anyhow!("entry `{}` disappeared — this is a bug", entry_name))?;

        let outcome = ctx.core.exec.engines.run(self.default_mode, ctx, module, entry);
        // defer-class-initialization: 懒执行的 `__static_init__` 抛出时无法就地传播
        // （触发点是返回 `Option` 的查找函数），改为记录首个失败并在这里转成运行错误——
        // 与变更前 boot 期 `init_static_fields` 的 `bail!` 同等响度。程序自身的错误优先。
        match outcome {
            Err(e) => Err(e),
            Ok(()) => match ctx.take_static_init_error() {
                Some(msg) => bail!("{msg}"),
                None => Ok(()),
            },
        }
    }

    /// Strict entry resolution (no fallback chain). The hint **must** be
    /// supplied — either from the zpkg `Entry` field (baked at compile
    /// time by `PackageCompiler.BuildTarget.AutoDetectEntry`) or from the
    /// CLI positional `[entry]` argument on `z42vm`.
    fn resolve_entry_in(module: &Module, hint: Option<&str>) -> Result<String> {
        let name = hint.ok_or_else(|| anyhow::anyhow!(
            "no entry point: pass the function name as the second positional \
             argument to `z42vm`, or rebuild with `z42c build` which bakes \
             an entry into the zpkg automatically (define a `Main()` in source)"
        ))?;
        if !module.functions.iter().any(|f| f.name == name) {
            bail!("entry function `{}` not found in module `{}`", name, module.name);
        }
        Ok(name.to_string())
    }
}
