# z42.build

## 职责
z42 项目「编译 → 发布」管线的**框架库**（全 z42 实现）。提供：固定的相位流程引擎、
传给各相位的 `IPipelineContext` 契约、以及 workload 与项目 `build/` 脚本继承扩展的
基类（`WorkloadBase` / `BuildHooks`）。

**不做**：平台相关实现 —— 那住在各 workload 的 `*.workload.zpkg`
（见 `src/toolchain/workload/`），通过 `: WorkloadBase` 子类提供。

> ⚠️ **Parked / 接口先行**：本目录**接口 + 流程 + 骨架实现**齐备，但**实现体均为桩**
> （`PipelineContext` 成员、`Pipeline` head 相位仅描述流程，无具体逻辑）。
> 故意不建 `z42.build.z42.toml` 清单，**暂不接入编译**（未登记 workspace / xtask / CI）。
> 具体的 `IPipelineContext` 落地、原生 builtin（sign/archive/hash/probeVersion/download）、
> `ICompiler` 的 z42c 实现适配均留待深加工后按 spec-first 正式落地。
>
> **计划重构（后面）**：把编译相关接口（`ICompiler` + CompileRequest/CompileResult）
> **独立到一个中立微库**，让编译器核心（z42c）与编排器（z42b）都只依赖该微库，而非整个
> build 框架。当前为减少 churn 暂置本库；落地写 `build-orchestrator.md` 时入 Deferred + roadmap 索引。

## 设计要点（三层继承链）
```
项目 build/        ──►  workload 平台实现       ──►  z42.build 基类
class iOSBuild         class iOSWorkload            class WorkloadBase
  : iOSWorkload          : WorkloadBase               (no-op 默认)
  override + base.X()     override 平台逻辑            扩展点契约
```
- 继承 / 重载 / hook 全归一到 `override` + `base.M(ctx)`（z42 原生 OOP，**不需反射**）。
- 绑定不靠运行时动态加载：publish 时生成一次性 driver 程序，把
  `z42.build` + workload + 项目 `build/` 静态链接编译后运行（类似 `build.rs`）。
- **编译走 in-process 共享实现**（`ICompiler`）：Compile 头相位不 fork `z42c` 子进程，
  而是经注入的 `ICompiler` 在进程内调编译器库——与独立 `z42c.driver` CLI **同一份实现**。
  依赖倒置：z42.build 定接口，编译器库（z42c）`: ICompiler` 实现它（z42c → z42.build，无环）。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Pipeline.z42` | 管线驱动 —— **流程**：八相位顺序，head（z42.build 拥有）+ tail（workload 拥有） |
| `src/WorkloadBase.z42` | 平台尾相位扩展点（Preflight/Configure/GenerateProject/NativeBuild/Package） |
| `src/BuildHooks.z42` | 平台无关头相位 hook 扩展点（Before/After × Compile/Trim/Assets）+ `ProvideNative`（专用窄相位：产本包私有 native 库，add-native-dep-config） |
| `src/IPipelineContext.z42` | **相位上下文契约**：项目模型 + 能力受限 fs + exec + 日志 + 产物登记 + 平台原语 + preflight 原语 |
| `src/ICompiler.z42` | **编译抽象**：Compile 头相位经此**在进程内**调编译器库（不 fork z42c）；`z42b` 与 `z42c.driver` 引用同一实现。含 CompileRequest / CompileResult 记录 + NoCompiler 兜底。**计划后续抽到中立微库**（见下） |
| `src/PipelineContext.z42` | `IPipelineContext` 的 SDK 实现（骨架）—— 受限 fs / exec / 平台原语 / 产物登记的落地点。编排器构造它注入 ctx。**归属暂置本库**（2026-06-23 决策） |
| `src/Models.z42` | 管线运行期记录（Target/Dirs/Inputs/Output/ExecResult；项目模型在 z42.project） |
| `src/BuildKinds.z42` | 常量：TargetFamily / BuildMode / Phase（用 const，避开 enum） |

## 入口点
- `Pipeline.Run(ctx)` —— 由编排器（`z42b`）/ driver 构造（注入 Compiler/Hooks/Workload）后调用
- `ICompiler` —— 编译抽象；编排器注入编译器库实现（与 `z42c.driver` 同一份），Compile 相位 in-process 调用
- `WorkloadBase` / `BuildHooks` —— workload 与项目的继承扩展点
- `IPipelineContext` —— 相位与外界交互的唯一契约

## 依赖关系
- 依赖 `z42.project`（项目清单模型 ProjectInfo / ProjectManifest / PlatformSet + typed 平台配置）
- 依赖 `Std.IO`（数据/进程语义参考）、`Std.Collections`（List）
- **被依赖**：编译器库（z42c）`: ICompiler` 实现 `ICompiler`（z42c → z42.build，仅接口；
  无环）；编排器 `z42b`（`src/toolchain/builder/`）依赖本库 + 该实现。
- **待补（实现期）**：`IPipelineContext` 中 Sign / Archive / Hash / ProbeVersion / Download
  对应的原生 builtin（toolchain 侧 Rust 实现）；`ICompiler` 的 z42c 实现适配。
