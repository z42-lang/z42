# Proposal: REPL 多行输入 + fn/class 顶层声明累积（add-repl-decls-multiline）

## Why

现有 REPL（0.4.0）跨轮只能累积 **变量**（`Vars{N}` 静态字段 carry-forward）与 `using`。相对 Python
交互式体验，缺两个核心机制：

1. **多行输入**：`fn f() {` … `}` / `class C {` … `}` 天然跨行，但宿主 [interactive_main.z42:28](../../../../src/toolchain/interactive/core/interactive_main.z42) 只调 `Repl.ReadLine`（单行）。
   底座（`__repl_readblock` 带括号平衡检测 + extern `Std.Repl.ReadBlock`）**已存在但未接线**。
2. **fn/class/type 顶层声明累积**：目前无法在 REPL 里定义函数或类型并在后续轮使用——这是与 Python
   差距最大的机制（design doc 自列为 follow-up：「fn/class 顶层声明累积」未接）。

不做则 REPL 只能当计算器用，无法交互式构建程序，与「REPL 作为 0.4.0 capstone 产品能力」的定位不符。

## What Changes

- **多行输入**：宿主 read 循环从 `Repl.ReadLine(">>> ")` 改为 `Repl.ReadBlock(">>> ", "... ")`，
  未闭合 `()[]{}` 时用续行提示符续读，读满一个括号平衡块再交给 `Script.Eval`。
- **声明累积**：`Script.Eval` 新增「顶层声明」输入类别（`class`/`struct`/`record`/`enum`/`interface`
  关键字开头，或 `<type> <name>(` 自由函数形状）。声明轮：把声明 emit 进本轮 `Repl.R{N}`、并入
  `CachedScan`（复用现成 `DepScan.ExtendWithPackage`）、把 `Repl.R{N}` 登记进「活跃声明命名空间集」；
  后续每轮 prelude 对该集逐个 `using Repl.R{N};`，使先前定义的 fn/class 可被引用。
- **同名重定义 → 报错**：`ScriptState` 记已声明符号名；重名声明返回错误、会话不推进（MVP 不做
  supersede；见 Out of Scope）。
- 文档：REPL 设计页（`docs/design/toolchain/repl.md`）状态模型/follow-up 段更新；scripting/interactive
  两目录 README 功能索引；roadmap Deferred 索引登记 `_`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/interactive/core/interactive_main.z42` | MODIFY | read 循环 `ReadLine`→`ReadBlock`（多行块）；`.help` 文案补多行/声明说明 |
| `src/toolchain/scripting/src/Script.z42` | MODIFY | `_classify` 扩顶层声明识别；`Eval` 加声明轮分支（emit + ExtendWithPackage + 登记 ns + 重名检测）；prelude 追加活跃声明 ns 的 `using` |
| `src/toolchain/scripting/src/ScriptState.z42` | MODIFY | 新增 `DeclNamespaces`（活跃声明命名空间列表）+ `DeclNames`（已声明符号名，用于重名检测） |
| `src/toolchain/scripting/tests/repl-decls/source.z42` | NEW | 声明累积 + 跨轮引用 + 重名报错的 [Test] 用例（若该目录测试形态适用；否则并入 e2e 夹具，见 tasks） |
| `src/tests/zbc-format/repl-multiline/source.z42` | NEW | 多行块求值端到端用例（占位；实际测试位置由 tasks 阶段 1 勘定后回填 Scope） |
| `docs/design/toolchain/repl.md` | MODIFY | 状态模型/输入分类/follow-up 段：加多行 + 声明累积；`_` 移入 Deferred |
| `docs/spec/changes/add-repl-decls-multiline/` | NEW | 本 change 的 proposal/spec/design/tasks |
| `src/toolchain/scripting/README.md` | MODIFY | 功能索引：加「顶层声明累积」入口 |
| `src/toolchain/interactive/README.md` | MODIFY | 功能索引：加「多行输入」入口 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加 `repl-future-underscore-var` 索引行 |

**只读引用**（理解上下文必须读，不修改）：

- `src/toolchain/scripting/src/Rewriter.z42` — 裸引用改写现状（声明名是否需改写的判定依据）
- `src/toolchain/scripting/src/Engine.z42` / `Repl.z42` — extern `ReadBlock` / `Invoke` 现状
- `src/runtime/src/corelib/repl.rs` — `__repl_readblock` 括号平衡语义（不改）
- `src/compiler/z42c.pipeline/src/DepScan.z42` / `PackageCompile.z42` — `ExtendWithPackage` / `CachedScan` 契约

## Out of Scope

- **`_` 上次结果变量**：因 z42 静态类型与 `object` 槽的取舍单独裁决，defer → `repl-future-underscore-var`。
- **fn/class 同名重定义（supersede）**：MVP 报错；supersede（后轮覆盖 + 旧 ns 剔除）留 `repl-future-redefine`。
- **JIT/interp 求值模式参数**（`--mode` / `.mode`）：独立 change B。
- **反射类元指令 `.type`/`.members`、Tab 补全、持久历史/`.load`**：各自既有 deferred 条目，不动。
- **VM/编译器源改动**：本 change 复用现成 builtin 与编译原语，不改 `src/runtime/` / `src/compiler/`。

## Open Questions

- [ ] 声明名（fn/class 名）是否需要经 `Rewriter` 改写？初判**不需要**——它们在 `Repl.R{N}` 命名空间里、
      靠 `using` 引入活跃集，裸引用由 `GetStaticScoped` 按活跃 ns 集解析（与变量的 `Vars{N}.x` 限定不同）。
      阶段 1 实测确认。
- [ ] 新测试的落点：scripting `tests/`（[Test] 单元）vs `src/tests/`（VM e2e 夹具）。阶段 1 勘定后回填 Scope。
