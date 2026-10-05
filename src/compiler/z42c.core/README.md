# z42c.core

## 职责
编译器基础设施层：源码位置 `Span`、诊断 `Diagnostic` / `DiagnosticBag`、语言特性开关 `LanguageFeatures`、协议名常量 `ProtocolNames`。命名空间 `Z42.Core`。无兄弟依赖，被编译器前后端引用。

本包是 **host-platform-independent 可移植前端**，供 z42c 编译器与 scripting / playground / runtime 共享；不属于 `Std.*` 标准库 API 面，只是恰好与 stdlib 同处 build + ship。冷启动破环预建见 [self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md) 轴 ④。受限写法（无 enum / 类字段无泛型 / List 约束 → typed array）同见该文。

## 功能索引
| 功能 | 入口 |
|------|------|
| 源码位置范围 | `Z42.Core.Span` |
| 诊断构造 / 收集 | `Diagnostic`、`DiagnosticBag`（`Add` / `Error` / `ErrorCount` / `HasErrors`）|
| 错误码常量 | `DiagnosticCodes`（E01xx–E10xx）|
| 特性开关查询 | `LanguageFeatures.IsEnabled` / `Has` / `Phase1Profile` |

## 如何测试验证
```bash
./xtask test stdlib z42c.core    # tests/diag.z42 + tests/features.z42
```
`xtask test compiler` 只认 `tests/<unit>/*.z42.toml` 目录单元，不覆盖本包的扁平 `tests/*.z42`。

## 关联文档
- [self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)
- 错误码：[error-codes.md](../../../docs/internals/src/compiler/error-codes.md)

## 待办
- DiagnosticRenderer / Catalog / Category（CLI 渲染）与 PreludePackages 尚未在 z42 侧实现，driver 需要时补

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Span.z42` | 源码位置范围 `[Start,End)` + 行列 + File |
| `src/DiagnosticSeverity.z42` | Error/Warning/Info（int 常量；z42 暂无 enum）|
| `src/Diagnostic.z42` | 单条诊断（Severity/Code/Message/Span + IsError + Format + 工厂）|
| `src/DiagnosticBag.z42` | 诊断收集器（typed array + count）|
| `src/DiagnosticCodes.z42` | E01xx–E10xx 错误码常量 |
| `src/LanguageFeatures.z42` | 特性开关（snake_case 名 + 并行数组）|
| `src/ProtocolNames.z42` | 协议（运算符 / 迭代等）名字常量 |

## 依赖关系
无（叶子）。stdlib 自动可用。
