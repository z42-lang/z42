# Proposal: unify-value-types —— 统一值类型模型（消灭 Z42PrimType）

## Why

z42c 编译器的类型系统现在有**二元割裂**：基元（`int`/`long`/`bool`/`char`/`float`/`double`）是
`Z42PrimType`（纯名字包装、无元数据），用户 struct 是 `Z42ClassType(IsStruct=true)`（完整元数据 +
StructLayout 字节布局）。这个割裂带来三个具体代价：

1. **七张并行的名字桥接映射**（`Canon` / `_canonPrim` / `_primWrapper`×2 / `_intPrimFQ` / `PrimTag` /
   `_isPrim`×3 / `_isPrimKeyword`）本质是同一张"关键字↔Std.* 值类型"表的七个投影（canonical 名 / 包装短名 /
   FQ 名 / IR 标签 / 是否基元），各自维护、语义还不完全一致（`_canonPrim` 保留 `int` 而 `Canon` 归一到
   `i32`）——是漂移与 bug 温床。
2. **降级 sentinel 反模式**：`ImportedSymbolLoader.z42` 在信息不全时把导入类型降级成 `Z42PrimType` 哨兵
   （philosophy.md「修复必须从根因出发」点名的反模式）。
3. **概念债**：`Std.Int32` 等基元 wrapper **已作为真实 phantom struct 存在**于 z42.core
   （`struct Int32 : INumber<int>`，零字段、`this`=裸标量），Int32.z42 注释明说"语义类型系统仍用
   `Z42PrimType("int")` 作身份"——即二元割裂是**历史妥协**，基础设施早已就绪可以消除它。

消除二元后：`int` ≡ `Std.Int32`（CLR 模型），类型解析/可赋性/重载/装箱/反射全部由"该类型的值类型模型
（是不是值类型 + 表示 Repr + 字节布局 + 引用位图）"统一驱动，七表收敛成一个入口。这也为后续的 FFI 值类型零
marshaling、单标量叶子塌缩、Value 密度压缩铺平道路。

## 程序全景（伞程序 `unify-value-types`，分阶段多 PR 交付）

> **交付纪律（bootstrap-seed 铁律 + workflow 单逻辑单元）**：整个统一是一个概念程序，但**必须按有序
> 阶段多 PR 交付**——任何 format bump 的阶段走 support-先行/晚一 nightly-再 use；每阶段独立 DRAFT →
> GREEN → PR。本 proposal 覆盖全程；**本 change 首个 PR 只实施 Phase 1**（下方 Scope 即 Phase 1）。

| 阶段 | 内容 | 触面 | 格式 bump | 本地可验 |
|------|------|------|----------|---------|
| **Phase 1（本 PR）** | R1 消灭 Z42PrimType（int→Std.Int32 Z42ClassType）+ R2 Repr{Scalar,Blob} 形式化 + 七表收敛 + R4 算术零回归 + 消 ImportedSymbolLoader sentinel | 纯编译器 `z42c.semantics` 单子包 | ❌ 无 | ✅ warm，self-host byte-identical 门禁 |
| Phase 2 | R3 装箱统一（按 P4b 现状重设计：基元 `Value::Boxed` vs struct 堆 `BoxedStruct` 不对称的收敛） | runtime | 待定 | 部分 |
| Phase 3 | R5 FFI 值类型 marshaling（struct blob 按 layout 字节直传 native） | runtime marshal | 可能 | 部分 |
| Phase 4 | 单标量叶子 struct 塌缩（GCHandle 类）+ R7 runtime 谓词收敛（`primitive_class_name`/`is_integer_class`/`prim_isa` 归一到 StructLayout 单一来源） | runtime | 可能 | 部分 |

## What Changes（Phase 1）

- **R1 类型解析统一**：`SymbolTable.ResolveTypeP` 对基元关键字改产 `Std.*` 的 `Z42ClassType(IsStruct=true)`
  （从符号表查已存在的 phantom struct），不再 `new Z42PrimType(...)`。删除 `Z42PrimType` 类。
- **R2 Repr 形式化**：值类型带 `Repr∈{Scalar,Blob}`。Scalar = 六个基元 wrapper（零字段、由原生 `Value::I64/
  F64/Bool/Char` 承载）；Blob = 多字段/含引用叶子 struct（现 StructLayout 机制）。
- **七表收敛**：`Canon`/`_canonPrim`/`_primWrapper`/`_intPrimFQ`/`PrimTag`/`_isPrim`/`_isPrimKeyword` 收敛成
  单一"关键字→值类型（canonical 名 + Repr + IR 标签 + FQ）"入口。
- **R4 算术零回归**：`_emitBinary` 一行不动；`EmitContext.ToIrType` 学会对 Scalar 值类型返回 `PrimTag(Std名)`。
- **装箱路由（编译器侧）**：`BoxIfNeeded`/`_emitBox` 的 `is Z42PrimType` 分支改按 Repr——Scalar 整数→
  `__box_prim`（runtime 不动）、Blob→`__box_struct`（runtime 不动）；bool/char/double/string 不装箱特例
  **保留**（有独立 Value 变体自带类型身份）。**Phase 1 不碰 runtime 装箱**。
- **消 sentinel**：`ImportedSymbolLoader` 降级路径改根因修（导入类型解析到正确值类型，不再产 `Z42PrimType` 哨兵）。
- **codegen-output-preserving 不变式**：Phase 1 是编译器内部类型模型的纯重构，**必须 emit 逐字节相同的
  IR/zbc**——self-host gen1==gen2 byte-identical + 全量 golden 不变是**正确性门禁**。

## Scope（允许改动的文件，Phase 1）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | 删 `Z42PrimType` 类；`Canon` 升级为"关键字→Std.* canonical/FQ"；`_canWiden` 数值拓宽迁到统一值类型可赋性 |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | `ResolveTypeP` 基元分支改产 `Std.*` Z42ClassType；`_isPrim`/`_canonPrim` 收敛进统一表 |
| `src/compiler/z42c.semantics/src/EmitContext.z42` | MODIFY | `ToIrType` 对 Scalar 值类型返回 `PrimTag`；`_primWrapper` 收敛 |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | `BoxIfNeeded` 的 `is Z42PrimType` 改 Repr 判定；`_intPrimFQ` 收敛（bool/char/double 特例保留） |
| `src/compiler/z42c.semantics/src/StructLayout.z42` | MODIFY | 加 `Repr(Scalar/Blob)` 形式化；把基元 wrapper 纳入值类型模型；供 R2 判定入口 |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | 消除 `Z42PrimType` 降级 sentinel（根因修：解析到正确值类型） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | 字面量 typing 产出改产 Std.* 值类型（int/long/char/string/bool/内插/拼接） |
| `src/compiler/z42c.semantics/src/TypeFactsTc.z42` | MODIFY | float/double 字面量产出改；`_primWrapper`/`_isPrimKeyword`/`_isNumericPrim` 收敛；prim↔prim 窄化判定改 |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | `.Length`/`.Count`→int、枚举常量→long 产出改；`_primWrapper` 派发路径随收敛 |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | Object 基类方法签名合成产出改（object/ToString→string/Equals→bool/GetHashCode→int） |
| `src/compiler/z42c.semantics/src/BinaryTypeTable.z42` | MODIFY | 算术结果类型产出改（double/float/long/int/bool） |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | `_isStructArg` 从 `is Z42PrimType` 改"IsStruct 值类型" |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | 形参 `PrimTag` 判别从 `is Z42PrimType` 改 Repr |
| `src/compiler/z42c.semantics/src/Bound.z42` | MODIFY | `BoundIsExpr` 结果 bool 类型产出改 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | packed 数组元素判别（`at.Elem is Z42PrimType`）+ `_emitBox` 透传判别改按 Repr；`_emitBinary` 不动 |
| `src/compiler/z42c.semantics/src/PrimModel.z42` | NEW | 单一「关键字↔Std.* 值类型」表（收敛七表；design 已批"新辅助"）——阶段 1 已落 |
| `src/compiler/z42c.semantics/tests/primmodel/prim_model_tests.z42` | NEW | PrimModel 单测（独立单元，依赖 z42.ir 用 IrType）——阶段 1 已落 |
| `src/compiler/z42c.semantics/tests/primmodel/z42c.semantics.test.primmodel.z42.toml` | NEW | 上述单元的构建配置 |
| `src/compiler/z42c.semantics/tests/types/type_tests.z42` | MODIFY | `new Z42PrimType(...)` 机械替换/删除 |
| `src/compiler/z42c.semantics/tests/bound/bound_tests.z42` | MODIFY | 同上 |
| `src/compiler/z42c.semantics/tests/map/map_tests.z42` | MODIFY | 同上 |
| `src/compiler/z42c.semantics/tests/overload/overload_tests.z42` | MODIFY | 同上 |
| `docs/book/src/runtime/value-type-model.md` | NEW | 统一值类型模型机制页（Repr/Scalar/Blob/七表收敛/codegen 不变式） |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新页 |
| `docs/roadmap.md` | MODIFY | unify-value-types 程序进度 + 后续阶段 Deferred 索引 |

**只读引用**（理解上下文，不改）：

- `src/libraries/z42.core/src/Primitives/*.z42` — 基元 phantom struct 定义（Int32/Int64/Boolean/Char/Double/Single）
- `src/compiler/z42c.semantics/src/OverloadResolver.z42` — 重载键靠 `Canon` 归一，保持 Canon 语义即透明不改
- `src/runtime/src/interp/exec_vcall.rs` — 确认 runtime 已默认把裸 I64 路由 Std.Int32（Phase 1 不改 runtime）
- `docs/design/runtime/object-abi.md` — Value 密度压缩是独立 Deferred，不在本程序范围
- `docs/spec/archive/2026-08-09-add-struct-value-semantics/design-radical.md` — 原始架构 DRAFT（行号已过时，本 proposal 以当前 main 为准更新）

## Out of Scope（Phase 1）

- **runtime 任何改动**（`src/runtime/`）——Phase 1 纯编译器；R3 装箱统一 / R5 FFI / R7 runtime 谓词收敛属 Phase 2-4。
- **格式 bump**（zbc/zpkg）——Phase 1 codegen-output-preserving，无格式变化。
- **单标量叶子 struct 塌缩**（用户 `struct Id{int v}` → 标量）——Phase 4。
- **Value 24→16B / NaN-box / 引用压 8B**——object-abi.md 独立 Deferred，非本程序。
- **新语法 / 新关键字**——无；`int`/`long` 源码写法不变（只是内部解析目标变了）。

## Open Questions

- [ ] **codegen 不变式可达性**：R1/R2 是否能真正做到 self-host byte-identical？风险点=重载解析对 phantom
  struct 的候选选择、方法 mangle 键、TYPE/反射元数据。缓解=self-host byte-identical 是硬门禁，任何漂移即
  bug 修到不动点（或如过往 opt 改动走 D7 一代自愈）。**需在实施中以 GREEN 验证，DRAFT 阶段无法先验证。**
- [ ] **Repr 判定的确切谓词**：Scalar ⟺ 六个基元 wrapper（有原生 Value 变体承载）——用户零字段 struct（退化）
  与单字段 struct 都算 Blob（塌缩留 Phase 4）。design.md D2 细化，请 User 确认。
- [ ] **bool/char/double 不装箱特例**：保留（有独立 Value 变体）。design.md D5 确认。
