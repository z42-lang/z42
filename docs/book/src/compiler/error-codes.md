# 错误码体系

> **页型**: 参考页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `src/libraries/z42c.core/src/`（`Diagnostic.z42` / `DiagnosticBag.z42` / `DiagnosticCodes.z42`）
> **相关**: [源代码编译流程](source-compile.md) · [架构总览](architecture.md) ｜ **对齐**: 2026-09-11

## 概述

编译期问题以**诊断码**表达（`E####` 错误、`W####` 警告），按 pipeline 阶段分段编号，由编译器收集后统一报告。运行期错误不再用码，而是抛出类型化异常（`Std.*Exception`），调用方按类 catch。

## 诊断结构

编译器用 `DiagnosticBag` 累积诊断，一次编译收集多条错误/警告再统一报告，而非首错即停。每条诊断是一个 `Diagnostic`：

| 字段 | 含义 |
|------|------|
| `Severity` | 级别：`Error` / `Warning` / `Info` |
| `Code` | 诊断码，如 `E0202` |
| `Message` | 人类可读描述 |
| `Span` | 源码位置 |

级别与码相互独立：码本身以 `E` / `W` 前缀区分错误与警告，`Severity` 决定是否阻断编译。

## 错误码分段

码号按产生它的 pipeline 阶段分段：

| 段 | 阶段 | 示例 |
|----|------|------|
| `E01xx` | 词法 | `E0101` 字符串未闭合、`E0103` 非法数字字面量 |
| `E02xx` | 语法 | `E0201` 意外 token、`E0202` 期望某 token、`E0203` 意外 EOF |
| `E03xx` | 特性门控 | `E0301` 使用了当前阶段未启用的特性 |
| `E04xx` | 类型检查 | 类型不匹配、未定义标识符（`E0401`）、未定义类型（`E0443`）、重载歧义等（本段最密集） |
| `E05xx` | IR 生成 | `E0501` 代码生成阶段错误 |
| `E06xx` | 包 / 导入解析 | `E0601` 导入符号冲突；`W0603` / `W0604` 导入相关警告 |
| `E09xx` | Native / 测试 | `[Native]` FFI 约束；`[Test]` 家族的位置 + 签名强制（`E0911`/`E0912`/`E0915`，见下） |
| `E10xx` | 调用实参绑定 | `E1001` / `E1002` 参数绑定错误 |

> **`E0911` / `E0912` / `E0915`（测试 attribute 强制）**：`[Test]` / `[Benchmark]` /
> `[Setup]` / `[Teardown]` 必须是**零接收者**（顶层自由函数或 `static` 方法）、**返回 `void`**、
> **无参数**、**非泛型**、**有方法体**。由
> [`DeclEnforcer._passTestAttrEnforce`](../../../../src/compiler/z42c.semantics/src/DeclEnforcer.z42)
> 强制（纯语法，挂在 `SymbolCollector` 三个入口，与 D8 后缀 pass 并列）。规则来源是 runner 的调用
> 契约——`Std.Test.Runner` 按 TIDX 全限定名**无参**调用，实例方法的 receiver 对不上。
> **实参语义**：`E0914`（`[Skip]` 的 `reason` 必填非空；`[Skip]`/`[Ignore]` 须与 kind attr 同贴）、
> `E0917`（`[Timeout]` 的 `milliseconds` 必填且为正）与位置/签名同 pass；`E0913`
> （`[ShouldThrow<E>]` 的 `E` 须派生 `Exception`）需符号表走基类链，落在相邻的语义相 pass
> `_passTestAttrSemantic`。`E` 在符号表中解析不到时**刻意不报**（跨包/表不完整会误伤）。

> `E0203`（意外 EOF）除标示语法错，还兼作 REPL **可恢复不完整**信号：parser 在「缺 token 且当前 token
> 为 EOF」时置 `DiagnosticBag.IncompleteAtEof` 并报此码，REPL 完整性探针 `Completeness.IsIncomplete`
> 据此判「输入没写完、需续读」。机制见 [REPL 输入完整性判定](../toolchain/repl-input-completeness.md)。

完整码表以 `DiagnosticCodes.z42` 的常量为准（每个码是一个命名常量，如 `ExpectedToken = "E0202"`）。

## 运行期错误

运行期错误**不用错误码**：VM 抛出类型化的 z42 异常（如 `Std.InvalidMarshalException`），调用方 `catch` 具体异常类后读 `Message` / `StackTrace`。这样运行期错误走语言自身的异常机制，与编译期诊断码是两套独立体系。

## 新增错误码

在 `DiagnosticCodes.z42` 对应阶段段内加一个命名常量（选段内下一个空号），产生诊断处引用该常量。错误用 `E` 前缀、警告用 `W`；`Severity` 按是否阻断编译选 `Error` / `Warning`。
