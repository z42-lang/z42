# examples

## 职责

z42 示例集合，分三类职责：

1. **语言 showcase**（单文件 `.z42`）— 新接手者快速理解语法面貌
2. **工程化模板**（`workspace-*/`）— 用户学多 member / 跨包依赖布局；同时被 `xtask test example`（`scripts/test/xtask_test_example.z42`）回归校验，保证 shipped 模板不腐烂
3. **SDK 嵌入示例**（`embedding/`）— 随 SDK 包分发给用户的全平台 hello world + 嵌入 API 用例

> 注：纯测试夹具不放这里。例如 TIDX round-trip 用的 test_demo 已挪到
> `src/runtime/tests/data/test_demo/`（只被 `zbc_compat.rs` 消费，不是给人读的示例）。

## ⭐ 全平台 hello world（embedding/）

[`embedding/hello_c/`](embedding/hello_c/) 是 z42 的招牌：**一份 byte-identical 的
`main.c`，链不同平台的 lib 就能跑在 desktop / wasm / iOS / Android**。
[`embedding/hello_c/README.md`](embedding/hello_c/README.md) 一份覆盖全部四个平台的编 + 链命令；
打包脚本把它注入每个平台的 SDK 包。[`embedding/hello_rust/`](embedding/hello_rust/) 是同流程的
Tier 2 Rust 版本。

| 目录 | 演示主题 |
|------|---------|
| `embedding/hello_c/` | Tier 1 C ABI 嵌入：全平台 byte-identical `main.c` + `hello.z42` |
| `embedding/hello_rust/` | Tier 2 Rust 嵌入：同一 hello-world 流程，宿主更 ergonomic |
| `embedding/multi_line.z42` | 多行字符串嵌入演示 |

## 单文件示例

| 文件 | 演示主题 | 当前状态 |
|------|---------|---------|
| `hello.z42` | 最小程序：`namespace`、`using`、字符串插值、`Greet` 函数 | ✅ Phase 1 可跑 |
| `types.z42` | 基本类型与变量（sbyte/byte/short/int/long、float/double、char、string、bool） | ✅ Phase 1 可跑 |
| `oop.z42` | 类、接口、`record`、继承 | ✅ Phase 1 可跑 |
| `patterns.z42` | `enum` 与模式匹配 / discriminated union | ✅ Phase 1 可跑 |
| `string_members.z42` | `string.Length` / `string.IsEmpty`（M7 stdlib 成员演示） | ✅ Phase 1 可跑 |
| `exceptions.z42` | `try / catch / throw` 异常处理（Phase 2 将替换为 `Result<T, E>` + `?`） | ✅ Phase 1 可跑 |
| `generics.z42` | 泛型类、Lambda、委托、LINQ 风格 | 🚧 设计目标；泛型基础已落地（L3-G1/G3a/G4h），完整跑通需 L3 收尾 |
| `params_varargs.z42` | `params` 变长参数：normal/expanded 调用形态 + `params object[]` 混类型装箱 | 🚧 parser/typechecker 单测已覆盖等价形态；端到端执行待 driver 下次自举 |

> ✅ = 当前编译器/VM 可端到端执行；🚧 = 部分特性可用，整体待完善。
>
> 注：另有 `lambda` / `local_fn` / `closure_capture` / `named_args` / `raw_string_basic`
> 等可跑单文件 demo 未逐一列表，直接看源文件即可。

### 运行单文件示例

```bash
# 编译为 .zbc，再用 VM 执行
dotnet run --project src/compiler/z42.Driver -- examples/hello.z42 --emit zbc -o /tmp/hello.zbc
cargo run --manifest-path src/runtime/Cargo.toml -- /tmp/hello.zbc
```

> `examples/hello.z42.toml` 是单文件项目清单的演示（z42.toml 格式参考）。

## Workspace 模板

`workspace-*` 演示多 member、跨包依赖、policy、include preset 等工程化场景。
每个目录有自己的 `z42.workspace.toml`（顶部注释说明目的）。

| 目录 | 演示主题 |
|------|---------|
| `workspace-basic/` | 最小 workspace：virtual manifest + members glob + default-members + 共享元数据 + 集中 dependencies |
| `workspace-full/` | 多 member 跨依赖：`hello` → `utils` → `core` 三层 lib + app 混合 |
| `workspace-with-policy/` | `[policy]` 强制策略 + `[workspace.build]` 集中产物布局 |
| `workspace-with-presets/` | `[include]` 机制：member 显式拉取共享 preset 配置块 |

### 运行 workspace 示例

```bash
cd examples/workspace-basic
dotnet run --project ../../src/compiler/z42.Driver -- build --workspace
```

> workspace 工程化命令（`info` / `build --workspace` / `build -p <member>`）
> 由 C4 提案逐步落地，部分子命令可能仍在路上 —— 各 `z42.workspace.toml`
> 顶部注释会标明所需里程碑。

## 关联文档

- 语言整体语法：[../docs/design/language/language-overview.md](../docs/design/language/language-overview.md)
- 工程文件 / 项目格式：[../docs/design/compiler/project.md](../docs/design/compiler/project.md)
- Phase / 里程碑路线：[../docs/roadmap.md](../docs/roadmap.md)
