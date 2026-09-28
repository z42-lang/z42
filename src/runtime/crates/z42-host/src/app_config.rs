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
    // 显式 `Z42_APP_CONFIG` 优先，解析不出内容则回落到 app 旁边的侧车。判据与理由见
    // `config::load_app_config_tables` —— `z42vm` 的 `main()` 与这里共用**同一份**实现
    // （曾是两份、两种语义，见那里的头注）。
    let app = match z42::config::load_app_config_tables(&getenv, Some(std::path::Path::new(file))) {
        Ok((rt, _)) => rt,
        Err(e) => {
            // 库路径：绝不 exit（可能跑在宿主进程里）。
            eprintln!("z42: {e}\n     -> app sidecar ignored; other layers still apply.");
            None
        }
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
