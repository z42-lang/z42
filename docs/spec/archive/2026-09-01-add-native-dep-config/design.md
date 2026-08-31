# Design: `[native]` 声明面 + build-hook 产 native + 传递复制

## Architecture

```
                          ┌─ z42.repl（lib，携带 native）──────────────────────┐
z42.repl.z42.toml         │  [build] hooks = "hooks"                           │
  [native.z42_repl]       │  [native.z42_repl]                                 │
  repl/hooks/hooks.z42 ───┼─► ProvideNative(ctx):                             │
                          │     cargo build -p z42-repl（同 VM 的裸 cargo）    │
                          │     cp libz42_repl.<suf> → ctx.Dirs.Dist/<rid>/    │
                          │     ctx.AddOutput("native", <path>)                │
                          └────────────────────────────────────────────────────┘
                                        │ z42.repl/dist/<rid>/libz42_repl.<suf>（自包含）
                                        ▼
z42.interactive（exe，path-dep z42.repl）
  z42 publish → builder_publish:
    _pubBundleProjectDeps        → 拷闭包 zpkg（已有）
    _pubBundleProjectNativeDeps  → 走同一闭包，对声明 [native] 的 dep：
        载入 dep hooks → ProvideNative（dep-scoped ctx, Target.Rid=目标 rid）
        取 dep dist/<rid>/libX.<suf> → 平铺进 payload programs/z42i/libX.<suf>
                                        │
                                        ▼
  <sdk>/programs/z42i/{z42.interactive.zpkg, libz42_repl.<suf>}
  运行期 resolve_native_beside(programs/z42i/, "z42_repl") —— 不变
                                        │
xtask packaging: [component.z42i] 整目录拷 publish 输出（native 随行，零特殊处理）
```

## Decisions

### Decision 1: `[native]` 跨平台 schema —— 约定式（rid 目录 + 派生文件名）

**问题**：一处声明一个 native 库，语言无关、跨平台统一，怎么组织最简？

**选项**：
- A（约定）：`[native.<name>]` 空表，文件约定 `<dist>/<rid>/lib<name>.<平台后缀>`。一行覆盖全平台。
- B（显式 per-rid map）：`[native.<name>]` 下逐 rid 列文件路径。全通用但冗长、易漏平台。
- C（混合）：A 默认 + B 显式覆盖。

**决定：选 A**（B 作 Deferred 逃生口）。理由：
- z42 全线平台身份是 **rid**；文件名由平台派生 `<prefix><name><suffix>`（`<prefix>`=`DLL_PREFIX`：unix `lib`、
  Windows 空；`<suffix>`=`.dylib`/`.so`/`.dll`）——rid 定子目录、前缀/后缀派生，天然"一行统一跨平台"，对齐
  `resolve_native_beside` 契约与 `_cargoOutFor` 的按-target-triple 分目录。
- 唯一真实消费者 `z42_repl` 完美贴合约定；预编译库只要放到 `<dist>/<rid>/<prefix>z42_repl<suffix>` 同样适用
  （语言无关）。显式 per-rid 覆盖属投机（memory 纪律：无真实消费者不做）→ Deferred。

**schema（最小形态）**：
```toml
[native.z42_repl]
# 本包携带 native 库 z42_repl；文件约定 <dep-dist>/<rid>/<prefix>z42_repl<suffix>
# （<prefix>=DLL_PREFIX，Windows 空、其他 lib；<suffix>=.dylib/.so/.dll）。
# 有 [build] hooks 的 ProvideNative 就现场产出；无 hook 则视为已提交的预编译文件。
```
per-lib 用表头 `[native.<name>]`（当前可空），为将来 per-lib 选项（`static` / `files` 覆盖）留位，不必现在加。

**文件名 = 平台派生 `<平台前缀><name><后缀>`（User 2026-09-01 裁决，"Windows 统一"的正解）**：
`[native]` 只写**逻辑 name**；实际文件名由**平台派生**——前缀用平台原生前缀（unix `lib`、**Windows 空**）、
后缀用平台族（`.dylib`/`.so`/`.dll`），即 Rust `std::env::consts::DLL_PREFIX`/`DLL_SUFFIX` 语义。所以
Windows = `z42_repl.dll`（无 `lib`）、unix = `libz42_repl.so`。**三处（config 约定 / 生产端拷贝 / 运行期
`resolve_native_beside`）共用这一条派生规则**——这才是"Windows 统一"：不是强制加 `lib`，而是**同一派生
规则覆盖所有平台**，Windows 自然落位。**`resolve_native_beside` 保持不变**（它本就用 `DLL_PREFIX`/
`DLL_SUFFIX`）；生产端 z42 侧放一个**同语义**的派生工具（前缀/后缀按 rid 族），与 Rust 端对齐。

### Decision 2: native 产出用专用相位 `BuildHooks.ProvideNative`，不复用 BeforeAssets

**问题**：消费者遍历闭包要"跑 dep 的 hook 产 native"，但盲跑 dep 的通用 `BeforeAssets` 可能触发任意副作用。

**决定**：给 `BuildHooks` 加**专用窄相位** `ProvideNative(ctx)`（默认 no-op，与已有 `WorkloadBase.NativeBuild`
命名同族）。消费者只调这个。z42.repl 的 `ProjectHooks` override 它。顶层 publish 自身的 native（若有）也走同相位。
→ 语义单一、可安全传递跑、缓存友好（`ctx.Exec` 已标脏）。

### Decision 3: cargo 与 VM 同源（裸 cargo + 同 rid→target 映射）

**问题**：hook 编 z42-repl 的 cargo 要"和虚拟机保持一致"。

**事实**：VM 构建用**裸 `cargo`**（`new Process("cargo")`），versions.toml `[toolchain.rust]` 只在入口
`versions_check_rust` 校 channel/min_version，不提供 cargo 路径。

**决定**：hook 同样用 `ctx.Exec("cargo", …)`（裸 cargo，PATH 解析），`--manifest-path src/runtime/Cargo.toml
-p z42-repl`，rid→target 用与 `_cargoTargetFor` 一致的规则（host=空 target 复用 warm 构建、cross=`--target
<triple>`）。z42-repl 是 runtime workspace 成员 → 落进 VM 已 warm 的 `artifacts/build/runtime/<profile>/`，
hook 再拷进 `Dirs.Dist/<rid>/`。**与 VM 完全同源、同 rid 语义**，不引入第二套 toolchain 解析。

### Decision 4: 传递复制 = 已有闭包遍历 + `[native]` 声明驱动，不硬编码

**问题**：z42.interactive 怎么知道要拷 z42.repl 的 native？

**决定**：不硬编码。`_pubBundleProjectNativeDeps` 复用 z42.interactive 已走的 path-dep 闭包
（`PathDepPlan.Resolve` / `_pubBundleProjectDeps` 同一套遍历），对每个声明 `[native]` 的 dep：
1. 定位 dep toml → `_resolveDistDir(depPm, depDir)` 得 dep dist；
2. 若 dep 有 `[build] hooks` → 载入并跑其 `ProvideNative`（dep-scoped ctx，Target.Rid=publish 目标 rid，
   Dirs.Dist=dep dist）——保证 native 已产出（warm 幂等）；
3. 取 `dep-dist/<rid>/lib<name>.<suffix>` → 平铺进消费者 payload（`programs/z42i/lib<name>.<suffix>`）。
   移动端改拷 OS lib 目录（骨架，桌面先行）。

### Decision 5: rid 分目录（staging）↔ 运行期平铺（payload）

- **staging/dist 树**：`<dist>/<rid>/lib<name>.<suffix>` —— 按 rid 分，交叉编译产物不撞（你要的 `dist/rid/xxx`）。
- **运行期/payload**：native-libraries.md §2 铁律"运行期布局唯一=平铺、无 rid 子目录"。publish 挑**目标 rid**
  那一份拍平到 payload 旁。两者一致，不冲突。

## Implementation Notes

- **`IPipelineContext` 已够用**：`Target.Rid`（目标 rid）/ `Manifest`（读 `[native]`）/ `Exec` / `Dirs.Dist`
  可写（写护栏放行 Intermediate/Dist）/ `AddOutput`。无需给 ctx 加字段。
- **`ProjectManifest.Natives`** 用"构造后填"（仿 `_parseAnalyzers`/`_parseLints`），不动构造函数签名。
- **平台后缀派生**：z42 侧按 rid 族映射（macos→`.dylib`/`libX`，linux→`.so`/`libX`，windows→`.dll`/`X`），
  与 Rust `DLL_PREFIX`/`DLL_SUFFIX` 语义一致。放一处工具函数（NativeSpec 或 builder helper）。
- **删 xtask 特殊处理后**：`_pkgBuildAndStageRuntime` 不再编 z42-repl；libz42_repl 也不再进共享 cargoOut 的
  desktop 扫描路径 → `_copyNativeLibs` 的 repl 排除分支变多余，一并删。

## Testing Strategy

- **单元**：`ManifestLoader._parseNative` 解析 `[native.<name>]`（含缺省、多库）→ `ProjectManifest.Natives`。
- **单元**：平台后缀派生函数（各 rid 族 → `lib<name>.<suffix>`）。
- **e2e**：一个 fixture lib 带 `[native]` + 假 ProvideNative（产一个 sentinel .so），一个 exe path-dep 它 →
  publish 后断言 native 平铺进 payload 且 rid 正确。
- **集成回归**：`xtask build toolchain` 产 z42i → `programs/z42i/libz42_repl.*` 存在 + z42i REPL 冒烟 `1+1=2`
  （替代原 `_pkgStageReplCdylib` 路径，证明删特殊处理后仍工作）。
- **GREEN**：`xtask test` 全 stage + z42c 自举 gen1==gen2 不动点（本 change 不碰 z42c codegen，应天然不动）。
- **packaging**：`xtask package sdk` + `xtask test dist`（本地）/ 交 CI 冷路径。

## 两-nightly / bootstrap 核查（关键）

- **新 stdlib API**：`ProjectManifest.Natives` + `NativeSpec` + `BuildHooks.ProvideNative`。
- **谁消费**：`builder_publish`（z42b，toolchain，由**自建** z42c+stdlib 编）+ z42.repl hook（publish 期由注入
  编译器编）。
- **种子只编 xtask源 + z42c源**（ci-bootstrap 步 2/3）：二者**均不读** `.Native` / 不调 `ProvideNative`
  （packaging 脚本调 z42b publish，不解析 `.Native`；z42c 只读 deps/sources）。
- **z42.project / z42.build 源自身**加字段/相位 → 由种子 z42c 编（新字段/新 virtual，无新语法）→ 种子 z42c 能编。
- **结论**：预计 **单 PR** 可落（无 support/use 拆分）。**实施第一步必做**：`grep -rn "\.Natives\|ProvideNative"
  src/compiler scripts/*.z42` 确认无种子域消费；若命中 → 退回 support/use 两 PR（PR-1 加字段+相位发 nightly，
  PR-2 消费+接入+删 xtask）。

## Deferred / Future Work

### native-config-future-explicit-files: 显式 per-rid 文件覆盖
- **来源**：本 design Decision 1。
- **触发原因**：约定 `<dist>/<rid>/lib<name>.<suffix>` 覆盖 hook 产出 + 规范命名的预编译库；任意文件名 vendor
  blob 需显式路径。当前无此消费者。
- **前置依赖**：出现一个带非常规命名预编译 native 的真实库/app。
- **触发条件**：该消费者出现时，加 `[native.<name>] files."rid" = "path"` 解析 + 消费。
- **当前 workaround**：把预编译库按约定命名放到 `<dist>/<rid>/lib<name>.<suffix>`。

### native-config-future-static-link: 静态链接 native
- **来源**：本 design Out of Scope。
- **触发原因**：本 change 只做动态 dlopen colocation；静态 `.a` 链进 apphost 是另一条路径（deployment-model E 轴）。
- **触发条件**：需要把 native 静态并进单文件 exe 时。
