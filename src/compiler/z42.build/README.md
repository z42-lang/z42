# z42.build

## 职责
z42 项目「编译 → 发布」管线的**框架库**（全 z42 实现）。提供：固定的相位流程引擎、
传给各相位的 `IPipelineContext` 契约、以及 workload 与项目 `build/` 脚本继承扩展的
基类（`WorkloadBase` / `BuildHooks`）。

**不做**：平台相关实现 —— 那住在各 workload 的 `*.workload.zpkg`
（见 `src/toolchain/workload/`），通过 `: WorkloadBase` 子类提供。

## 功能索引
- `Pipeline.Run(ctx)` —— 由编排器（`z42b`）/ driver 构造（注入 Compiler/Hooks/Workload）后调用
- `ICompiler` —— 编译抽象；编排器注入编译器库实现（与 `z42c.driver` 同一份），Compile 相位 in-process 调用
- `WorkloadBase` / `BuildHooks` —— workload 与项目的继承扩展点
- `IPipelineContext` —— 相位与外界交互的唯一契约

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

## 如何测试验证
本库无独立 `tests/`，随编译器 workspace 构建，编译通过即接口契约成立；`ICompiler` 的实现由 `z42c.pipeline` 的测试覆盖：
```bash
./xtask build compiler        # 编译器域全部包（含本库）
./xtask test compiler         # z42c 自举不动点 + smoke
```

## 关联文档
- 设计 / 机制：[z42b.md](../../../docs/internals/src/toolchain/z42b.md)、[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)
- 编排器：[`src/toolchain/builder/`](../../toolchain/builder/README.md)

## 待办
- `IPipelineContext` 的 Sign / Archive / Hash / ProbeVersion / Download 对应的原生 builtin（toolchain 侧 Rust 实现）；`PipelineContext` 的 Exec 与平台原语仍为 extern / stub
- 编译相关接口（`ICompiler` + CompileRequest/CompileResult）独立到中立微库，使编译器核心与编排器只依赖该微库而非整个 build 框架

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Pipeline.z42` | 管线驱动 —— **流程**：八相位顺序，head（z42.build 拥有）+ tail（workload 拥有） |
| `src/WorkloadBase.z42` | 平台尾相位扩展点（Preflight/Configure/GenerateProject/NativeBuild/Package） |
| `src/BuildHooks.z42` | 平台无关头相位 hook 扩展点（Before/After × Compile/Trim/Assets）+ `ProvideNative`（专用窄相位：产本包私有 native 库） |
| `src/IPipelineContext.z42` | **相位上下文契约**：项目模型 + 能力受限 fs + exec + 日志 + 产物登记 + 平台原语 + preflight 原语 |
| `src/ICompiler.z42` | **编译抽象**：Compile 头相位经此**在进程内**调编译器库（不 fork z42c）；`z42b` 与 `z42c.driver` 引用同一实现。含 CompileRequest / CompileResult 记录 + NoCompiler 兜底。**计划后续抽到中立微库**（见下） |
| `src/PipelineContext.z42` | `IPipelineContext` 的 SDK 实现（骨架）—— 受限 fs / exec / 平台原语 / 产物登记的落地点。编排器构造它注入 ctx。**归属本库** |
| `src/IReplCompiler.z42` | **REPL 编译门面**：opaque 会话句柄 + 粗粒度查询（`ReplCompileResult`）；`Std.Scripting` 只依赖本接口，实现 `Z42cReplCompiler` 在 `z42c.pipeline`，运行期经反射注入 |
| `src/BuildLog.z42` | 构建进度输出去向开关（进程级，进度走 stderr 以保 `--format json` 的 stdout 纯净） |
| `src/Models.z42` | 管线运行期记录（Target/Dirs/Inputs/Output/ExecResult；项目模型在 z42.project） |
| `src/BuildKinds.z42` | 常量：TargetFamily / BuildMode / Phase（用 const，避开 enum） |

## 依赖关系
- 依赖 `z42.project`（项目清单模型 ProjectInfo / ProjectManifest / PlatformSet + typed 平台配置，path 依赖）、`z42.core`、`z42.io`
- **被依赖**：`z42c.pipeline`（`Z42cCompiler : ICompiler`、`Z42cReplCompiler : IReplCompiler`，仅接口、无环）、
  `z42.scripting`（`IReplCompiler`）、编排器 `z42b`（`src/toolchain/builder/`）与各 workload appbuilder。
