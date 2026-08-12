# Spec: 跨包 struct 值语义（P4a）

## ADDED Requirements

### Requirement: 跨包使用 struct 值类型

包 B `import` 包 A 定义的多字段 `struct` 后，B 对该 struct 的构造/字段读写/方法/传参/返回按**值语义**工作，
与 struct 定义在 B 本地时**行为一致**——消费编译器把 imported struct 正确分类为 blob 值类型。

#### Scenario: 跨包构造 + 字段读写 + 方法
- **WHEN** 包 A 定义 `public struct Point { public int x; public int y; Point(int,int){...} int Sum(){...} }`，包 B `A.Point p = new A.Point(1,2); p.x = 5; p.Sum();`
- **THEN** B 的 z42c 把 `A.Point` 识别为 blob 值类型（`IsStruct`），发 `StructAlloc`/`StructFieldGetPrim/SetPrim`（烘焙字节 offset，与 A 侧一致）→ 运行 `p.x==5`、`Sum()==p.x+p.y`，**不崩** blob-bounds

#### Scenario: 跨包传参 copy-in + 返回值语义
- **WHEN** 包 B 把 `A.Point` 传给函数 / `A.Point q = p; q.x = 42;`
- **THEN** 值复制语义成立（`q.x=42` 不动 `p.x`；传参改副本不动原值），与本地 struct 一致

#### Scenario: 跨包嵌套 struct 字段
- **WHEN** 包 A 定义 `struct Line { Point a; Point b; }`，包 B 用 `line.a.x` / `line.a.x = 100`
- **THEN** 消费编译器对 `A.Line` 与其内嵌 `A.Point` 均分类为 struct，重算的复合字节布局与 A 侧逐字节一致 → 叶子读写正确、兄弟字段独立

#### Scenario: 消费方重算布局与生产方持久化布局一致
- **WHEN** 消费编译器 `StructLayout.BuildFromSymbols` 对 imported struct 从字段名/类型重算布局
- **THEN** 重算的 `size` + 字段 offset + 引用位图与生产方 zpkg TYPE 段持久化的 `StructSize`/ref bitmap **逐字节相同**（共享确定性 `_compute`）——否则复现 blob-bounds 崩，故 golden 端到端守住

## MODIFIED Requirements

### Requirement: imported 类型的 struct 分类

**Before:** `ImportedSymbolLoader` 造 imported `Z42ClassType` 时从不设 `IsStruct`（默认 false）→ 所有 imported
struct 被消费编译器当引用类型 → 跨包 struct 运行期 blob-bounds 崩。`ExportedClassZ` 无 `IsStruct`，struct-ness
仅隐式为 `HasBase=false`。

**After:** `ExportedClassZ` 带显式 `IsStruct`；`TsigReconcile` 从 `cd.Flags & CLASS_FLAG_STRUCT(4)` 设它；
`ImportedSymbolLoader` `nct.IsStruct = cl.IsStruct` → imported struct 正确分类为 blob 值类型，走现有 struct
发射路径。

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式变更（复用现有 `StructAlloc`/`FieldGetPrim/SetPrim` + 现有 TYPE 段 struct
布局块 `Flags bit2` + 字段名/类型）。纯编译器内部标志传播。**格式中立。**

## Pipeline Steps

- [ ] Lexer / Parser / AST — 不涉及
- [x] **符号加载（TypeChecker 前）** — `ImportedSymbolLoader` 设 imported struct `IsStruct`（根因）
- [x] **IR / TSIG** — `ExportedTypes` + `TsigReconcile` + `ExportedTypeExtractor` 传/设 `IsStruct`
- [ ] IR Codegen — 不改（分类正确后自动走现有 struct 发射路径）
- [ ] VM interp / JIT — 不涉及（跨包 struct 的 JIT 由已开 P5-A #175 覆盖；本 change 无关）
