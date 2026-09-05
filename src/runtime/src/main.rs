use anyhow::Result;
use clap::{Parser, ValueEnum};

// 启动期辅助（stdlib 定位 / tracing / panic hook / --info / 模块路径）——
// 见 startup.rs 顶部关于为什么拆开的说明。
mod startup;
use startup::*;

// z42c self-compile is malloc-bound (~31% in the system allocator, profiled).
// Route the z42vm process's global allocator through mimalloc — cuts z42c
// compile ~40% and alloc-heavy (string-building) workloads ~3×. Gated on the
// default-on `mimalloc-alloc` feature so wasm/mobile presets (built
// `--no-default-features`) fall back to the system allocator.
//
// `dhat-heap` (script-profiling P0, `xtask profile --heap`) replaces the global
// allocator with dhat's heap-profiling shim, so it is mutually exclusive with
// mimalloc — a build carries at most one `#[global_allocator]`. Only ever built
// on demand into a throwaway target-dir by `xtask profile`; never shipped.
#[cfg(all(feature = "mimalloc-alloc", not(feature = "dhat-heap")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[derive(Parser)]
#[command(name = "z42vm", about = "z42 Virtual Machine", version)]
struct Cli {
    /// 要执行的字节码：`.zbc`（单文件）或 `.zpkg`（工程包）。
    /// 自省命令（`--info` / `--list-knobs` / `--show-config`）下可省。
    file: Option<String>,

    /// 入口函数名，覆盖 zpkg 里烤好的 `Entry`。
    ///
    /// 平时不用给——`z42c build` 会把 `Main()` 烤进 zpkg。裸 `.zbc`（无 zpkg 元数据）
    /// 必须给。主要消费方是测试运行器：每个 `[Test]` fork 一次 `z42vm <file> <test>`。
    entry: Option<String>,

    // ── 执行 ────────────────────────────────────────────────────────────
    /// 执行模式，覆盖本次运行的默认值。
    #[arg(long, value_enum, help_heading = "执行")]
    mode: Option<ExecMode>,

    /// 详细日志（等价于 `--set log=z42=info`）。
    #[arg(short, long, help_heading = "执行")]
    verbose: bool,

    // ── 运行时配置 ──────────────────────────────────────────────────────
    /// 为本次运行设置一个运行时旋钮，可重复：`--set gc-mode=concurrent`。
    ///
    /// 优先级最高（压过 `Z42_*` 环境变量与配置文件）。旋钮名见 `--list-knobs`。
    #[arg(long = "set", value_name = "KEY=VALUE", help_heading = "运行时配置")]
    set: Vec<String>,

    /// 把来自环境变量 / 配置文件的配置问题从警告升级为致命错误。
    ///
    /// 命令行（`--set`）的问题一律致命，不受此开关影响。CI 用它把配置漂移变成硬失败。
    // 等价 `Z42_STRICT_CONFIG=1`。引入：complete-runtime-settings P1（2026-09-05）。
    #[arg(long, help_heading = "运行时配置")]
    strict_config: bool,

    // ── 自省 ────────────────────────────────────────────────────────────
    /// 构建信息 + 完整旋钮快照，然后退出。**提 bug 时贴这个。**
    // 引入：docs/review.md Part 4 D5（2026-05-25）。
    #[arg(long, help_heading = "自省")]
    info: bool,

    /// **有哪些旋钮**：类型 / 可设置层 / 本 build 的可用性 / 默认值，然后退出。
    #[arg(long, help_heading = "自省")]
    list_knobs: bool,

    /// **旋钮当前是什么值**、来自哪一层，以及某一层的值为什么没生效，然后退出。
    #[arg(long, help_heading = "自省")]
    show_config: bool,

    /// 让上面两个命令连不推荐 / 内部旋钮一起列出。
    #[arg(long, help_heading = "自省")]
    all: bool,

    /// 让上面两个命令输出 JSON 而非文本。
    #[arg(long, help_heading = "自省")]
    json: bool,

    // ── 诊断 ────────────────────────────────────────────────────────────
    /// 程序正常退出后，把运行时计数器打到 stderr。
    ///
    /// `--stats` = 人读的文本块；`--stats=json` = 单行 JSON（供工具抓取）。
    // 引入：docs/review.md Part 4 D6（2026-05-26）；JSON 形态来自 script-profiling P0。
    //
    // `require_equals`：可选值必须写成 `--stats=json`。不加的话 clap 会把紧跟其后的
    // 位置参数当成它的值——`z42vm --stats app.zpkg` 会报「invalid value 'app.zpkg'
    // for '--stats'」。这是带可选值的 flag 的通病，`--color=always` 同款解法。
    #[arg(long, value_name = "FORMAT", num_args = 0..=1, require_equals = true,
          default_missing_value = "text", help_heading = "诊断")]
    stats: Option<StatsFormat>,

    /// 传给被运行程序的参数（`--` 之后的一切）。z42vm 自己不解析它们，
    /// z42 代码经 `Std.IO.Environment.GetCommandLineArgs()` 读到。
    #[arg(last = true)]
    args: Vec<String>,
}

// 2026-05-11 retire-z-codes: `--explain` / `--list-errors` were removed
// alongside the Rust-side `diagnostics` catalog. Use `z42c explain E####`
// for compile-time codes; runtime errors are typed z42 exceptions now.

// 2026-05-07 add-runtime-feature-flags (P4.1): variants are feature-gated so
// `--help` only advertises modes the binary can actually run, and clap rejects
// unsupported `--mode jit` requests with a friendly enum-list error.
/// `--stats-format` selector (script-profiling P0). `Text` is the historical
/// human block; `Json` is a single-line object for tooling.
#[derive(Clone, PartialEq, Eq, ValueEnum)]
enum StatsFormat {
    Text,
    Json,
}

#[derive(Clone, ValueEnum)]
enum ExecMode {
    Interp,
    #[cfg(feature = "jit")]
    Jit,
    #[cfg(feature = "aot")]
    Aot,
}

/// `--mode` value as the knob's string form — used to report a `--mode` /
/// `--set mode=` conflict in the user's own words.
fn exec_mode_name(m: &ExecMode) -> &'static str {
    match m {
        ExecMode::Interp => "interp",
        #[cfg(feature = "jit")]
        ExecMode::Jit => "jit",
        #[cfg(feature = "aot")]
        ExecMode::Aot => "aot",
    }
}

/// Resolve the config-driven default execution mode (`Z42_MODE` / `[runtime].mode`,
/// unify-run-modes P2). Sits below the `--mode` CLI flag and above the build
/// default. Returns `None` to fall through to the build default when: unset, an
/// unrecognized value, or a feature-gated mode (jit / aot) not compiled into this
/// build — each non-empty invalid case warns once on stderr so it isn't silent.
fn resolve_config_mode(mode: Option<&str>) -> Option<z42::metadata::ExecMode> {
    match mode {
        None => None,
        Some("interp") => Some(z42::metadata::ExecMode::Interp),
        Some("jit") => {
            #[cfg(feature = "jit")]
            { Some(z42::metadata::ExecMode::Jit) }
            #[cfg(not(feature = "jit"))]
            { eprintln!("z42: Z42_MODE/[runtime].mode=jit but this build has no jit feature; using build default"); None }
        }
        Some("aot") => {
            #[cfg(feature = "aot")]
            { Some(z42::metadata::ExecMode::Aot) }
            #[cfg(not(feature = "aot"))]
            { eprintln!("z42: Z42_MODE/[runtime].mode=aot but this build has no aot feature; using build default"); None }
        }
        Some(other) => {
            eprintln!("z42: Z42_MODE/[runtime].mode={other:?} not recognized (interp/jit/aot); using build default");
            None
        }
    }
}

fn main() -> Result<()> {
    // Heap profiling (script-profiling P0, `xtask profile --heap`): the dhat
    // profiler must outlive the whole run — held here so it flushes `dhat-heap.json`
    // (in cwd) when it drops at main() exit. Only present in the `dhat-heap` build.
    #[cfg(feature = "dhat-heap")]
    let _dhat_profiler = dhat::Profiler::new_heap();

    let cli = Cli::parse();

    // Centralized runtime config — single source of truth for Z42_* env vars
    // consumed at boot (docs/review.md Part 4 D1, 2026-05-26). Subsystem
    // reads go through the process-global runtime_config().
    //
    // Precedence chain (unify-run-modes P0): env > [runtime] TOML file > default.
    // The optional config-file layer is named by Z42_CONFIG; a malformed file
    // is fatal (explicit error, never silent-default). Install the resolved
    // config as the global BEFORE tracing / subsystems read it, so the
    // [runtime] layer is visible everywhere. Z42_CONFIG unset → resolve(env,
    // None) == the previous env-only from_env() (non-breaking).
    let getenv = |n: &str| std::env::var(n).ok();

    // CLI layer (complete-runtime-settings P2) — parsed before anything reads the
    // config, since `--set log=...` has to be visible to init_tracing below.
    let cli_knobs = match z42::config::parse_set_args(&cli.set) {
        Ok(m) => m,
        Err(msg) => { eprintln!("{msg}"); std::process::exit(2); }
    };
    // `--mode` and `--set mode=` are the same precedence layer; refuse to guess.
    if let Err(msg) = z42::config::reject_flag_conflict(
        &cli_knobs, "Z42_MODE", "--mode", cli.mode.as_ref().map(exec_mode_name)
    ) {
        eprintln!("{msg}");
        std::process::exit(2);
    }

    // Two independent file layers: the user's own (`Z42_CONFIG`) and the app's
    // build-generated sidecar (`Z42_APP_CONFIG`). They merge per key with the user
    // winning — setting Z42_CONFIG must not discard what the app ships with.
    // `--all` / `--json` 只修饰两个自省命令。静默忽略是最坏的——用户以为拿到了 JSON。
    // 与 `--set` 未知 key 直接 exit 2 同一原则：命令行是「此刻手敲」的层，不猜、说出来。
    if (cli.all || cli.json) && !(cli.list_knobs || cli.show_config) {
        let which = if cli.json { "--json" } else { "--all" };
        eprintln!("z42: {which} only applies to --list-knobs / --show-config");
        std::process::exit(2);
    }

    let runtime_table = z42::config::load_runtime_toml(&getenv)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    // app-config 层：显式 `Z42_APP_CONFIG` 优先，否则由 app 文件推导出它旁边的
    // 侧车（app-config-follows-the-app）。推导必须发生在**装配**期——配置在
    // OnceLock 里 boot 后冻结，`app::run` 那时再想加一层就晚了。
    // app 侧车同时携带 `[runtime]`（旋钮）与 `[properties]`（应用自定义配置）。
    let (app_table, app_props) = {
        let explicit = getenv("Z42_APP_CONFIG").filter(|s| !s.trim().is_empty());
        let path = match explicit {
            Some(p) => Some(std::path::PathBuf::from(p.trim())),
            None => cli.file.as_deref()
                .and_then(|f| z42::config::sidecar_for(std::path::Path::new(f))),
        };
        match path {
            Some(p) => z42::config::load_config_tables(&p, "Z42_APP_CONFIG")
                .map_err(|e| anyhow::anyhow!("{e}"))?,
            None => (None, None),
        }
    };
    // 属性归 app 所有，不参与分层——用户配置里写了它是无效的，但静默忽略会让人
    // debug 半天（add-app-properties）。
    if runtime_table.is_some() {
        if let Ok(p) = getenv("Z42_CONFIG").ok_or(()) {
            if let Ok((_, Some(_))) = z42::config::load_config_tables(
                std::path::Path::new(p.trim()), "Z42_CONFIG") {
                eprintln!("z42: [properties] in Z42_CONFIG is ignored — application properties \
belong to the app and come only from its <app>.runtimeconfig.toml sidecar.");
            }
        }
    }
    let build_ctx = z42::config::BuildCtx::current();
    let inputs = z42::config::Inputs {
        cli: cli_knobs,
        user_config: runtime_table.as_ref(),
        app_config: app_table.as_ref(),
    };
    let (cfg, mut resolution) =
        z42::config::RuntimeConfig::resolve_with(&getenv, &inputs, &build_ctx);

    // Unknown keys are only reported for layers whose namespace z42vm owns — the
    // `[runtime]` table here, and `--set` keys (P2). Environment variables are NOT
    // scanned: the `Z42_` prefix is shared with the launcher, the test harness and
    // embedders, so "unknown knob Z42_HOME" would fire on every single run.
    for (table, layer) in [
        (runtime_table.as_ref(), z42::config::Layer::UserConfig),
        (app_table.as_ref(), z42::config::Layer::AppConfig),
    ] {
        let Some(table) = table else { continue };
        for key in z42::config::unknown_table_keys(table) {
            resolution.diagnostics.push(z42::config::unknown_key_diagnostic(layer, &key));
        }
    }

    // Severity split (complete-runtime-settings design.md Decision 3): CLI problems
    // are always fatal; env / config-file problems warn and fall back, unless strict
    // mode is on. Exit code 2 distinguishes "your configuration is wrong" from a
    // normal runtime failure (1).
    let strict = cli.strict_config
        || getenv("Z42_STRICT_CONFIG").and_then(|s| z42::config::parse_bool(&s)).unwrap_or(false);
    if let Err(msg) = resolution.into_result(strict) {
        eprintln!("{msg}");
        std::process::exit(2);
    }
    let resolution = resolution;

    let cfg = cfg.with_app_properties(app_props);
    if z42::config::init_runtime_config(cfg.clone()).is_err() {
        eprintln!("z42: runtime config already initialised before main(); [runtime] layer may be ignored");
    }

    init_tracing(cli.verbose, &cfg);
    install_panic_hook();
    #[cfg(unix)]
    z42::signal_handler::install();

    // Verbose-mode startup banner (docs/review.md D5 item 2, 2026-05-26).
    // One-line tracing::info — gated by EnvFilter / `--verbose` so the
    // default quiet boot is preserved. `--info` users get the full dump
    // from `print_build_info` instead.
    tracing::info!(
        "z42vm {} ({}) starting [pid={}]",
        env!("CARGO_PKG_VERSION"),
        build_feature_tag(),
        std::process::id(),
    );

    // Knob query surfaces (complete-runtime-settings P3). Like --info they run
    // before any module loading and do not need a <FILE>.
    if cli.list_knobs {
        print!("{}", if cli.json {
            z42::config::list_knobs_json(cli.all, &build_ctx)
        } else {
            z42::config::list_knobs_text(cli.all, &build_ctx)
        });
        return Ok(());
    }
    if cli.show_config {
        print!("{}", if cli.json {
            z42::config::show_config_json(&resolution, cli.all)
        } else {
            z42::config::show_config_text(&resolution, cli.all)
        });
        return Ok(());
    }

    // --info: print build info to stdout and exit before doing any module loading.
    if cli.info {
        print_build_info(&resolution, &build_ctx);
        return Ok(());
    }

    // Resolve module search paths (Z42_PATH + cwd + cwd/modules); log only for now.
    let module_paths = resolve_module_paths();
    if cli.verbose {
        log_module_paths(&module_paths);
    }

    // `file` is required when not in --info mode. clap can't express "required
    // unless --info" cleanly, so enforce it here.
    let file = cli.file.as_deref()
        .ok_or_else(|| anyhow::anyhow!(
            "missing required argument <FILE> (or pass --info / --list-knobs / --show-config)"))?;
    tracing::debug!("z42vm loading {}", file);

    // Locate stdlib libs directory.
    let libs_dir = resolve_libs_dir();

    // Publish the resolved dir into $Z42_LIBS so an in-process program (notably
    // the z42c compiler, which reads $Z42_LIBS directly for cross-package dep
    // resolution) sees the same libs dir the VM uses — SDK layout works with no
    // manual `Z42_LIBS=` set. Only fills an unset/empty var; explicit values
    // are respected.
    // "Did the user already specify libs?" must consult the RESOLVED knob, not the
    // raw env — otherwise `--set libs=/x` and `[runtime].libs` would be overwritten
    // by the auto-published value (adopt-inline-env-knobs, 2026-09-05).
    let user_libs = cfg.libs_dir.as_ref().map(|p| p.display().to_string());
    if let Some(val) = libs_env_to_publish(user_libs.as_deref(), libs_dir.as_deref()) {
        // Safety: z42 is single-threaded; no concurrent env reads during boot.
        unsafe { std::env::set_var("Z42_LIBS", &val); }
    }

    // Verbose libs-dir log (was inline before the former run sequence).
    if cli.verbose {
        match &libs_dir {
            Some(dir) => log_libs(dir),
            None => tracing::info!("libs dir: not found (set $Z42_LIBS or run package.sh)"),
        }
    }

    // Resolve effective execution mode ONCE: explicit `--mode` wins; else
    // config-driven (`Z42_MODE` / `[runtime].mode`, unify-run-modes P2); else the
    // build default (jit if compiled in, else interp). Feature-gated arms must
    // themselves be gated (constructor absent when the feature is off).
    let effective_mode: z42::metadata::ExecMode = match cli.mode {
        #[cfg(feature = "jit")]
        Some(ExecMode::Jit) => z42::metadata::ExecMode::Jit,
        #[cfg(feature = "aot")]
        Some(ExecMode::Aot) => z42::metadata::ExecMode::Aot,
        Some(ExecMode::Interp) => z42::metadata::ExecMode::Interp,
        None => resolve_config_mode(z42::config::runtime_config().mode.as_deref())
            .unwrap_or_else(|| {
                #[cfg(feature = "jit")]
                { z42::metadata::ExecMode::Jit }
                #[cfg(not(feature = "jit"))]
                { z42::metadata::ExecMode::Interp }
            }),
    };

    // Shared app-run core (add-embedded-app-run): load the app + execute its
    // entry — the same core the embedding path (z42-host::run_app) uses.
    z42::app::run(
        file,
        cli.entry.as_deref(),
        z42::app::RunOpts {
            mode: effective_mode,
            libs_dir,
            program_args: cli.args.clone(),
            print_stats: cli.stats.is_some(),
            stats_json: cli.stats == Some(StatsFormat::Json),
        },
    )
}

#[cfg(test)]
mod main_tests;
