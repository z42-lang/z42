//! 配置文件层的加载：用户配置（`Z42_CONFIG`）与应用侧车（`Z42_APP_CONFIG`）。
//!
//! # 两个文件层，不是一个
//!
//! 它们格式相同、解析器相同，区别只在**谁写的**：L3 是用户手写（"我这台机器上
//! 想改这个"），L4 由 build 生成、随产物分发（"这个应用需要这样跑"）。所以它们
//! **逐 key 叠加**（用户赢），而不是二选一。
//!
//! 修的是一个真实缺陷：launcher 从前用 `Z42_CONFIG` 一个通道同时表达两者，且只在
//! 用户没设时才塞侧车路径——于是用户一旦 `export Z42_CONFIG=my.toml`，应用自带的
//! `<app>.runtimeconfig.toml` 就被**整份丢弃**，而不是被逐 key 覆盖。
//!
//! # 只有 TOML 一种格式
//!
//! 不引入 JSON、也不为 JSON 留抽象层：unify-run-modes 已经裁决过把 .NET 风格的
//! JSON 侧车收编成 TOML（D5），z42 全仓配置格式已统一（`z42.toml` manifest /
//! `~/.z42/config.toml` / `.runtimeconfig.toml`）。为一个已被否决的方向留
//! `ConfigSource` trait 是付抽象税。唯一的关照是 `.json` 路径给一行**迁移提示**
//! 错误，而不是让从 .NET 迁来的人对着一个被静默忽略的文件 debug 半小时。
//!
//! complete-runtime-settings P4（2026-09-05）：自 config.rs 迁出并泛化为按路径读。

use std::path::{Path, PathBuf};

/// 读一个运行配置文件的 `[runtime]` 表。
///
/// - 文件不存在 → `Ok(None)` + 一行 warn（不致命：env / 默认仍然适用）。
/// - `.json` 后缀 → `Err`（迁移提示；**不**静默忽略）。
/// - 非法 TOML，或 `runtime` 存在但不是表 → `Err`（显式，绝不静默降级为默认）。
/// - 合法但没有 `[runtime]` 段 → `Ok(None)`。
pub fn load_config_file(path: &Path, var: &str) -> Result<Option<toml::Table>, String> {
    load_config_tables(path, var).map(|(rt, _)| rt)
}

/// 同 [`load_config_file`]，但同时取出 `[properties]` 段。
///
/// `[runtime]` 是 **VM 旋钮**（登记表校验、五层分层、未知键诊断）；`[properties]` 是
/// **应用自定义配置**——VM 不理解也不校验，只原样搬运给 `Std.Runtime.AppProperties`
/// （add-app-properties）。分成两张表而不是靠前缀区分：否则 `gc-mdoe` 这种 typo 会被
/// 当成一个合法的用户属性静默收下，「未知旋钮就明确报出来」那道诊断就死了。
pub fn load_config_tables(
    path: &Path,
    var: &str,
) -> Result<(Option<toml::Table>, Option<toml::Table>), String> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("app");
        return Err(format!(
            "{var}={} — z42 runtime config is TOML, not JSON.\n     \
             Use {stem}.toml with a `[runtime]` table (e.g. `[runtime]\\ngc-mode = \"concurrent\"`); \
             see docs/internals/src/runtime/runtime-settings.md.",
            path.display()
        ));
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // eprintln (not tracing) — this runs before the subscriber is installed.
            eprintln!("z42: {var}={} not found; ignoring that config layer", path.display());
            return Ok((None, None));
        }
        Err(e) => return Err(format!("{var}={}: {e}", path.display())),
    };
    let doc: toml::Table = toml::from_str(&text)
        .map_err(|e| format!("{var}={}: invalid TOML: {e}", path.display()))?;
    let runtime = match doc.get("runtime") {
        Some(toml::Value::Table(t)) => Some(t.clone()),
        Some(_) => return Err(format!("{var}={}: [runtime] must be a table", path.display())),
        None => None,
    };
    let props = match doc.get("properties") {
        Some(toml::Value::Table(t)) => Some(t.clone()),
        Some(_) => return Err(format!("{var}={}: [properties] must be a table", path.display())),
        None => None,
    };
    Ok((runtime, props))
}

/// 读 `var` 命名的配置文件层。变量未设 / 空 → `Ok(None)`（该层不存在）。
pub fn load_layer<F>(get: &F, var: &str) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    let Some(path) = get(var).filter(|s| !s.trim().is_empty()) else {
        return Ok(None);
    };
    load_config_file(Path::new(path.trim()), var)
}

/// 用户配置层（`Z42_CONFIG`）。历史名字，保留给既有调用方。
pub fn load_runtime_toml<F>(get: F) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    load_layer(&get, "Z42_CONFIG")
}

/// 应用侧车层的**原始读取**（只看 `Z42_APP_CONFIG`，不做回落）。
/// 装配 app-config 层请用 [`load_app_config_tables`]——回落判据在那里。
pub fn load_app_config<F>(get: &F) -> Result<Option<toml::Table>, String>
where
    F: Fn(&str) -> Option<String>,
{
    load_layer(get, "Z42_APP_CONFIG")
}

/// **app-config 层的唯一装配口**：显式 `Z42_APP_CONFIG` 优先，解析不出内容则回落到
/// `app_file` 旁边的侧车（[`sidecar_for`]）。同时取出 `[runtime]` 与 `[properties]`。
///
/// # 为什么判据是「解析出了内容」而不是「变量设了没有」
///
/// 侧车是 **app 的属性**，不是调用方的选项（见 [`sidecar_for`] 的头注）。而
/// `Z42_APP_CONFIG` 会**跨进程继承**：一个 z42 程序 spawn 出的子 app 会连带收到父 app
/// 的那份路径，而且它常是**相对路径**——子进程换个 cwd 就指向不存在的文件。
///
/// 此前 `z42vm` 的 `main()` 只看「变量非空」就锁死在显式分支，于是这种继承来的悬空值
/// 会把 app 自己声明的运行时设置**整份静默丢掉**，只留一行读起来像「无害地跳过一层」的
/// `not found` 提示。实测后果：`[runtime] probing-paths` 全失效 ⇒ 应用找不到自己的依赖
/// （GREEN 的 `probing-paths` e2e 格假红，2026-09-29）。
///
/// 嵌入入口（`z42-host::install_app_config`）本来就是按「解析出了内容」判的 —— 同一件事
/// 有过**两份实现、两种语义**，这里把它们收敛成一份。
pub fn load_app_config_tables<F>(
    get: &F,
    app_file: Option<&Path>,
) -> Result<(Option<toml::Table>, Option<toml::Table>), String>
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(p) = get("Z42_APP_CONFIG").filter(|s| !s.trim().is_empty()) {
        let tables = load_config_tables(Path::new(p.trim()), "Z42_APP_CONFIG")?;
        if tables.0.is_some() || tables.1.is_some() {
            return Ok(tables);
        }
        // 显式路径解析不出内容（文件缺失 / 既无 [runtime] 也无 [properties]）
        // → 不锁死，回落到 app 自己的侧车。
    }
    match app_file.and_then(sidecar_for) {
        Some(p) => load_config_tables(&p, "app sidecar"),
        None => Ok((None, None)),
    }
}

/// 读一个配置文件层，**问题一律降级为 warn**。
///
/// 给库入口（[`RuntimeConfig::from_env`]）用：它可能跑在宿主进程里，因一个配置
/// typo 杀掉宿主不是它该做的事。`z42vm` 的 `main()` 走 [`load_layer`] 的
/// `Result`，把同样的问题当致命处理。
///
/// [`RuntimeConfig::from_env`]: super::RuntimeConfig::from_env
pub fn load_layer_lenient<F>(get: &F, var: &str) -> Option<toml::Table>
where
    F: Fn(&str) -> Option<String>,
{
    match load_layer(get, var) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("z42: {e}\n     -> that config layer is ignored; env + defaults still apply.");
            None
        }
    }
}

/// app 旁边的运行配置侧车：`<同目录>/<同 stem>.runtimeconfig.toml`，存在才返回。
///
/// # 为什么约定住在这里
///
/// `z42c build` 把工程 `[profile.*]` 烤成这个文件放在 zpkg 旁边。此前**必须有人
/// 主动把路径设进 `Z42_APP_CONFIG`** 才算数——于是 `z42vm <app.zpkg>` 直跑、以及
/// 一切嵌入形态（wasm / iOS / Android / 桌面自包含）都拿不到 app 自己的运行配置，
/// 因为那些环境里根本没有"用户设环境变量"这回事。
///
/// dotnet 的 host 永远读 `<app>.runtimeconfig.json`，不需要谁指路——那是 **app 的
/// 属性**，不是调用方的选项。这个函数把 z42 拉回同一个模型：约定只有这一处实现，
/// 调用方**可以**传显式路径（`Z42_APP_CONFIG` 仍然优先），但不必自己去发现。
///
/// 现状：接上它的只有 z42vm（`main.rs`）与 Tier 2 `z42-host::run_app`；C 入口
/// `z42_host_run_app` 与 wasm 入口直接调 `app::run`，还不读侧车（runtime-settings.md 待办）。
///
/// 找不到是**常态**（多数工程没有 `[profile.*]` 运行时旋钮 ⇒ build 不产侧车），
/// 所以这里安静返回 `None`——与"显式指向一个不存在的文件"不同，那种情况
/// [`load_config_file`] 仍会 warn。
///
/// app-config-follows-the-app（2026-09-05）。
pub fn sidecar_for(app_file: &Path) -> Option<PathBuf> {
    // `with_extension` 替换最后一个扩展名：app.zpkg → app.runtimeconfig.toml
    //（不是追加），与 z42c 的产出名、launcher 的 `_runtimeConfigPath`、
    // publish 的 `_pubSidecarOf` 一致。
    let path = app_file.with_extension("runtimeconfig.toml");
    path.is_file().then_some(path)
}
