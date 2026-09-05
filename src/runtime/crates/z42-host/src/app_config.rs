//! app-config 层的装配 —— **嵌入入口**这一侧。
//!
//! `z42vm` 的 `main()` 在装配期做同一件事；这里是其余所有入口的对应物：桌面自包含
//! apphost、wasm、iOS、Android、testhost 全部经 [`crate::run_app`] 进来。没有这一步，
//! 工程在 manifest 里声明的运行时设置在这些形态上完全不生效——那些环境里也没有
//! "用户设环境变量"这回事。
//!
//! 拆成独立文件是行数硬限所迫（`lib.rs` 在 line-limit 棘轮基线上，越界文件不得增长）。
//! app-config-follows-the-app（2026-09-05）。

/// 把 app 旁边的 `<stem>.runtimeconfig.toml` 装进配置的 app-config 层。
///
/// `z42vm` 的 `main()` 在装配期做同一件事；这里是**其余所有入口**的对应物——
/// 桌面自包含 apphost、wasm、iOS、Android、testhost 全部经 [`run_app`] 进来。
/// 没有这一步，工程在 manifest 里声明的运行时设置在这些形态上完全不生效
/// （那些环境里也没有"用户设环境变量"这回事）。app-config-follows-the-app。
///
/// 必须在 [`z42::app::run`] **之前**：配置在 `OnceLock` 里 boot 后冻结。
/// 若已被装配（宿主自己先装过，或已有代码读过 `runtime_config()`）→ 保持不动，
/// 不覆盖调用方的选择。
pub(crate) fn install_app_config(file: &str) {
    let getenv = |n: &str| std::env::var(n).ok();
    let user = z42::config::load_layer_lenient(&getenv, "Z42_CONFIG");
    let app = match z42::config::load_layer_lenient(&getenv, "Z42_APP_CONFIG") {
        Some(t) => Some(t),
        // 显式未设 → 按约定找 app 旁边的侧车。
        None => z42::config::sidecar_for(std::path::Path::new(file))
            .and_then(|p| match z42::config::load_config_file(&p, "app sidecar") {
                Ok(t) => t,
                Err(e) => {
                    // 库路径：绝不 exit（可能跑在宿主进程里）。
                    eprintln!("z42: {e}\n     -> app sidecar ignored; other layers still apply.");
                    None
                }
            }),
    };
    let inputs = z42::config::Inputs {
        user_config: user.as_ref(),
        app_config: app.as_ref(),
        ..Default::default()
    };
    let (cfg, resolution) =
        z42::config::RuntimeConfig::resolve_with(&getenv, &inputs, &z42::config::BuildCtx::current());
    if z42::config::init_runtime_config(cfg).is_ok() {
        let _ = resolution.into_result(false);   // warn-only，同 from_env
    }
}
