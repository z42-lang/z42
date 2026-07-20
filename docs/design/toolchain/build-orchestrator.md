# 构建编排器 `z42b`

> ⚠️ **前瞻设计草案（未实施）**。`z42b` 驱动 [`z42.build`](../../../src/libraries/z42.build/) 管线，
> 编排「编译 → 发布」全流程。当前为 PARKED 骨架（`src/toolchain/builder/` + `z42.build`，
> 均无 toml、未接编译）。落地开 `docs/spec/changes/<name>/` spec，按 workflow 实施。
> **前置**：replace-csharp S5 完成（z42c 成生产编译器、`toolchain` 解锁）。

## 立柱与定位

`z42b` 是**构建编排器**：读 `z42.toml` / `--rid` → 构造并驱动 `z42.build` 八相位管线 → 产出
平台无关 `app.zpkg`（build）或平台交付件（publish）或 IDE 工程（export）。它**只编排 + 注入**，
不自己编译、不含平台实现。

与既有工具的分工（沿用 launcher「源 → zpkg → apphost」模式）：

```
launcher (z42)  ──分发──►  z42b  (src/toolchain/builder/core → z42.builder.zpkg → apphost z42b)
                             │  读 toml/--rid → 注入 Compiler/Workload/Hooks → Pipeline.Run
                             ▼
        z42.build 管线   head（z42.build 拥有）            tail（workload 拥有）
        Resolve → Compile → Trim → Assets        Configure → GenerateProject → NativeBuild → Package
                     │  经 ICompiler in-process 调编译器库          │  虚分发沿 项目→workload→基类
                     ▼                                              ▼
             z42c（: ICompiler，不 fork 子进程）        iOSWorkload / DesktopWorkload / ...
```

- **z42c = 编译器**（源 → app.zpkg），是 Compile 相位经 `ICompiler` **在进程内**调的库。
- **z42b = 编排器**（toml → 全流程，含 trim/assets/workload/打包）。
- 命令动词（build/run/publish/export/test、`--rid` 选平台）裁决见 [platform-export-lifecycle.md](platform-export-lifecycle.md)，本文不复述；本文聚焦 **z42b 内部如何编排**。

## In-process 编译：`ICompiler` 共享实现（核心设计）

Compile 头相位**不 fork `z42c` 子进程**，而是经 `z42.build` 的 `ICompiler` 接口**在进程内**调
编译器库——与独立 `z42c.driver` CLI **引用同一份实现**。

```
ICompiler (z42.build 定义)
   ├─ z42c.driver CLI     →  构造 CompileRequest → Compile()    （独立编译命令）
   └─ z42b Pipeline.Compile →  ctx 构造 CompileRequest → Compile()（编排内调用）
```

- **好处**：类型化请求/结果（非解析 stdout）、零进程开销、不依赖 z42vm 在 PATH。
- **依赖倒置（DIP）**：`z42.build` 定接口；编译器库 `: ICompiler` 实现 → z42c → z42.build（无环）。
- **换实现只动一处**：`z42b._hostCompiler()` 返回 `ICompiler` 实现；从骨架 `NoCompiler`
  切到真实 z42c 实现时，调用方（`Pipeline.Compile`）一行不改。

> **计划重构（Deferred，见下）**：`ICompiler` + CompileRequest/CompileResult 现暂置 `z42.build`，
> 后续抽到**中立微库**，使编译器核心（z42c）只依赖该微库、不依赖整个 build 框架。

## 项目 hook：动态注入（无代码生成，2026-07-20 落地）

| 路径 | 触发 | 机制 | 代价 |
|------|------|------|------|
| **标准** | 无 `[build] hooks` | **进程内组合**：`new Pipeline()` 注入 `_hostCompiler()` + 标准 workload → `Pipeline.Run(ctx)`。零子进程、零代码生成 | 无 |
| **带 hook** | manifest `[build] hooks = "<dir>"` | **动态注入**：用注入的**同一 `ICompiler`** 编 hook 目录 → `ModuleLoader.Load` → `Build.ProjectHooks` 反射实例化 → `as BuildHooks` → 挂 `p.Hooks` | 首次多编一次 hook 目录（极小，按缓存） |

> **实现取代原「生成一次性 driver」设计**：hook 类**直接注入进 z42b 自己的 `Pipeline`**，
> 无需为项目生成 / 编译 / 运行一个专属 driver——复用 `_hostCompiler` 同款组件注入模式
> （`ModuleLoader.Load` → `Type.GetType` → `Activator` → as-cast，见
> [dynamic-component-registration](../../spec/changes/dynamic-component-registration/)）。
> 依赖 [跨 zpkg 虚成员派发修复](../compiler/compiler-architecture.md#跨-zpkg-虚成员必须-vcall2026-07-20-fix-crosspkg-virtual-override)——hook 靠 override 依赖 zpkg 里的 `BuildHooks` +
> 读 `IPipelineContext` 属性运转，两条派发路径经该修复才生效。

## 自定义扩展：manifest 声明 hook 目录

**显式声明优于隐式探测**（避免把 xtask 的 `scripts/build/` 源码目录误判为 hook 目录）：
manifest `[build] hooks = "<dir>"`（projDir 相对）指向 hook 源目录；不声明则走标准路径。

```
myapp/
  z42.toml                   # [build] hooks = "hooks"
  src/                       # 应用代码
  hooks/                     # ← hook 源目录（manifest 显式声明）
    hooks.z42                #   namespace Build; class ProjectHooks : BuildHooks
```

- **注入约定**（`builder_hooks.z42`）：hook 源声明 `namespace Build;` + `class ProjectHooks :
  BuildHooks`，override 需要的钩子。z42b 编该目录 → `Type.GetType("Build.ProjectHooks")` →
  `Activator.CreateInstance` → `as BuildHooks` → `p.Hooks`。
- **合成入口**：`Z42cCompiler` 的目录编译面固定 `kind=exe`（auto-entry），hook 目录无 `Main`
  会判「no Main() found」；z42b 拷 hook 源到 staging 目录附一个合成 no-op `Main()` 再编
  （`ModuleLoader.Load` 不跑 Main，合成入口纯粹满足 exe 编译面）。
- **hook 编不过不阻塞主构建**：诊断打印后保守降级为无 hook（L1「空值 + 调用方检查」）。
- **平台 `<Family>Build` workload override 仍为设计未落**（当前只落 `Hooks` 注入）；落地时同款
  动态注入（`Type.GetType("Build.<Family>Build")` → `as WorkloadBase`）。

> 相位**封闭**（八个，线性，不可增删改序）；所有自定义落在 Hooks / Workload override 上，
> 不开放注册新相位（确定性 + 缓存模型）。

### 示例：编译前/后 hook

```z42
// hooks/hooks.z42 —— 平台无关的编译前/后扩展
namespace Build;                 // ← 固定命名空间约定（z42b 查 Build.ProjectHooks）
using Z42.Build;

public class ProjectHooks : BuildHooks {
    // 编译前：代码生成，把构建元数据写成 z42 源纳入本次编译
    public override void BeforeCompile(IPipelineContext ctx) {
        string gen = ctx.Dirs.Intermediate + "/gen/BuildInfo.z42";
        ctx.WriteText(gen,
            "namespace App; public static class BuildInfo {"
          + " public const string Rid = \"" + ctx.Target.Rid + "\"; }");
        ctx.Log("generated " + gen);
    }
    // 编译后（资产收集后）：例如校验/盖戳产物
    public override void AfterAssets(IPipelineContext ctx) {
        ctx.Log("post-assets check");
    }
}
```

### publish apphost：hook 免装 workload 产 stub（需求③）

`z42 publish`（desktop）需要一个 apphost stub（原生启动器壳）。默认由已装的 **desktop
workload** 提供（`z42 workload install desktop` 下载）。**项目 hook 可现场产出 stub**，从而
**去掉该下载依赖**——publish 的 stub 解析序：

1. **项目 hook 产出**：`[build] hooks` 声明 → `BeforeAssets` 经 `ctx.Exec` 现场编 stub →
   `ctx.AddOutput("apphost-stub", <path>)` 登记（`builder_publish.z42 _pubHookApphostStub`）
2. **`Z42_APPHOST_TEMPLATE`**：launcher 解析已装 workload 传入（Decision 3.5）
3. 皆无 → 报错提示两条路

本仓 xtask 即用此路：`scripts/hooks/hooks.z42` 的 `BeforeAssets` 现场 `cargo build --release`
出 apphost stub 并登记，`scripts/xtask.z42.toml` 声明 `[build] hooks = "hooks"` →
`z42 publish scripts/xtask.z42.toml` **免装 desktop workload** 直接产 apphost。

```z42
// scripts/hooks/hooks.z42
namespace Build;
using Z42.Build;

public class ProjectHooks : BuildHooks {
    public override void BeforeAssets(IPipelineContext ctx) {
        // cargo build --release 出 apphost stub（ctx.Exec = Std.Process）
        ExecResult r = ctx.Exec("cargo", /* build --release --manifest-path … */ args);
        if (r.ExitCode != 0) { ctx.Warn("cargo failed"); return; }  // 失败降级回 workload 路径
        ctx.AddOutput("apphost-stub", stubPath);                    // ← publish 解析序①取此
    }
}
```

```z42
// build/iOSBuild.z42 —— iOS 平台尾相位定制（override + base.X）
using Z42.Build;
using Z42.Workload;

public class iOSBuild : iOSWorkload {
    public override void Package(IPipelineContext ctx) {
        base.Package(ctx);                      // 先跑标准 .ipa 打包
        ctx.Log("custom post-package step");    // 再叠加自定义（额外签名/校验/上传准备）
    }
}
```

z42b 读到 `[build] hooks` 后编该目录并动态注入，等价于：
`new Pipeline{ Compiler=hostCompiler, Hooks=<注入的 ProjectHooks>, Workload=标准 }.Run(ctx)`
——`Hooks` 由 `ModuleLoader.Load` + `Activator` + `as BuildHooks` 现取，**无生成 driver**。

## `IPipelineContext` 实现归属

`PipelineContext`（`IPipelineContext` 的 SDK 实现：受限 fs / exec / 平台原语 / 产物登记）
**暂置 `z42.build` 库**（2026-06-23 决策），使编排器 / 注入的 hook 都能 `import` 它构造 ctx。
随 `ICompiler` 微库抽取一并重新审视分层。

**待补的 native 原语**（`IPipelineContext` 中，经 `extern` 接 toolchain 侧 Rust builtin）：
`Sign` / `Archive` / `Hash` / `ProbeVersion` / `Download`。

## 命名

- 框架库：**`z42.build`**（公共扩展 API，workload / 用户 `build/` 继承；属 `z42.<domain>` 族，
  不改 `z42b.core`）。
- 编排器包：**`z42.builder`**（`src/toolchain/builder/core/` → `z42.builder.zpkg`，二进制 `z42b`；
  与 `z42.launcher` 同构）。

## Decisions

| # | 决定 | 理由 |
|---|------|------|
| 1 | Compile 经 `ICompiler` **in-process** 调编译器库，不 fork z42c | 类型化、零进程开销、不依赖 PATH；与 z42c.driver 共享实现 |
| 2 | z42b 与 z42c.driver 引用**同一 `ICompiler` 实现** | 单一编译入口；换实现不动调用方 |
| 3 | 项目 hook 走**动态注入**（非生成 driver）；标准项目进程内零注入 | 复用 `_hostCompiler` 组件注入模式，无代码生成；仅带 hook 才付一次 hook 目录编译（2026-07-20 取代原「生成一次性 driver」设计）|
| 4 | hook 目录经 manifest `[build] hooks` **显式声明**（非隐式探测 `build/`）；命名空间约定 `Build.ProjectHooks` | 显式避免把 `scripts/build/` 等源码目录误判为 hook；`namespace Build` + 固定类名供 `Type.GetType` 反射定位 |
| 5 | 相位封闭，自定义只走 Hooks / Workload override | 确定性 + 缓存模型；不开放新相位 |
| 6 | `PipelineContext` 暂置 `z42.build`；`ICompiler` 后抽中立微库 | 减 churn；最终让编译器核心不依赖 build 框架 |
| 7 | 框架库 `z42.build` 不改名；编排器包 `z42.builder` | 框架是公共扩展 API（`z42.<domain>` 族）；包名同构 `z42.launcher` |
| 8 | 条件配置用**类型化轴子表 + 确定性合并**，**不引入 csproj `Condition` 表达式引擎** | 已知轴（profile/platform/rid）用 typed 段更优（类型化/可校验/顺序无关）；任意逻辑归 `build/` hooks（代码）。与 Decision #5「相位封闭」同一「声明式封闭 + 代码兜底任意逻辑」哲学。详见 [project.md 条件配置段](../compiler/project.md#条件配置类型化轴子表前瞻设计未实施) |
| 9 | **test / bench 宿主 = z42b**（非 z42c，2026-06-25 改）；runner 逻辑住 `z42.test`（库），z42b 是薄 verb | test/bench = build+run+report = 编排（对标 `dotnet test` 在 driver 非 `csc`）；on-device 面只能在 z42b（导出 harness+装设备）。Rust test-runner 同跑 [Test]+[Benchmark]，删它须同批替掉两者。修订 [retire-test-runner](../../spec/changes/retire-test-runner/) spec + roadmap 0.3.13/0.4.x |

## Deferred / 待 spec 细化

### z42b-future-icompiler-microlib: `ICompiler` 抽中立微库

- **来源**：本设计 / 2026-06-23 用户决策。
- **触发原因**：`ICompiler` 暂置 `z42.build`，致编译器库（z42c）实现它时传递依赖整个 build 框架。
- **前置依赖**：z42b 标准路径落地（确认 `ICompiler` 调用面稳定）。
- **触发条件**：正式落地 z42b（spec）时，或 z42c 侧适配 `ICompiler` 前。
- **当前 workaround**：接口暂留 `z42.build`，DIP 保证无环；interim 可接受。

### 其他待 spec 细化

- `build` 动词的停点（仅 head 跑、产 app.zpkg、不跑 workload tail）：用「不注入 workload
  （`WorkloadBase` no-op 兜底）」约定，还是给 `BuildMode` 加 `Build`（当前仅 Export/Publish，
  `Pipeline.Run` 仅在 Export 停于 GenerateProject）—— 落地 spec 时定。
- 条件配置组合轴若过于啰嗦：评估 cargo 式**有界 `cfg()` 谓词**（封闭 key 集 + `all/any/not`，
  仍可校验，非任意表达式）作为类型化轴子表的补充（见 [project.md 条件配置段](../compiler/project.md#条件配置类型化轴子表前瞻设计未实施)）。
- driver 生成的源码模板形态 + 输入 hash 缓存键设计。
- `PipelineContext` 各 native 原语（Sign/Archive/Hash/ProbeVersion/Download）的 Rust builtin 契约。
- 各 workload 现有 `export.z42` / `apphost.z42` 真实逻辑接进 `WorkloadBase` 相位的迁移。
- `[build]` 段是否需声明 hook（当前纯 `build/` 约定）；`[platform.*]` 完整 schema 见
  [platform-export-lifecycle.md](platform-export-lifecycle.md) Deferred。
