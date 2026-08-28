# z42c.core

## 职责
基础设施层（源码位置 Span / 诊断 Diagnostic·DiagnosticBag / 语言特性开关 LanguageFeatures）。命名空间 `Z42.Core`。无兄弟依赖，被编译器前后端引用。

> **位置（converge-z42-syntax-lib，route A 地基）**：本包是 **host-platform-independent 可移植前端**，已从 `src/compiler/` 挪进 `src/libraries/`，成 z42c 编译器**与** scripting/playground/runtime 共享的可移植库。**包名/命名空间不变**（仍 `z42c.core` / `Z42.Core`）——非 Std/z42.* 标准库 API 面，只是恰好与 stdlib 同处 build+ship。冷启动破环预建见 [self-hosting.md](../../../docs/design/compiler/self-hosting.md) 轴 ④。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Span.z42` | 源码位置范围 `[Start,End)` + 行列 + File |
| `src/DiagnosticSeverity.z42` | Error/Warning/Info（int 常量；z42 暂无 enum）|
| `src/Diagnostic.z42` | 单条诊断（Severity/Code/Message/Span + IsError + Format + 工厂）|
| `src/DiagnosticBag.z42` | 诊断收集器（typed array + count；Add/Error/Count/Get/ErrorCount/HasErrors）|
| `src/DiagnosticCodes.z42` | E01xx–E10xx 错误码常量（镜像 C# `DiagnosticCodes`）|
| `src/LanguageFeatures.z42` | 特性开关（snake_case 名 + 并行数组；IsEnabled / Phase1Profile / MinimalProfile）|
| `src/CoreSkeleton.z42` | **过渡占位**：尚未移植的 syntax/semantics/pipeline/driver 仍引用它；各自移植到真实 core 时移除 |

> 受限写法（无 enum / 类字段无泛型 / List 约束 → typed array）见 [self-hosting.md](../../../docs/design/compiler/self-hosting.md)。
> 测试：`tests/diag/`（11 例：诊断 7 + 特性 4），经 `xtask test compiler`。
> 待移植：DiagnosticRenderer·Catalog·Category（CLI 渲染，driver 需要时）/ PreludePackages。

## 入口点
`Z42.Core`（命名空间，镜像 C# 同名）。

## 依赖关系
无（叶子）。stdlib 自动可用。
