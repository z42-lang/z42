# Proposal: 补完泛型实例化模型（跨包 + 身份 + 密度）

> 类型：`lang` + `ir`（走阶段 1–9 完整流程）
> 前序：`generic-struct-erased-slot-value-copy`（PR #774，已归档 `archive/2026-09-23-…`）
> 落地方式：**一条线、三个 PR 顺序落**（User 裁决 2026-09-23）

## Why

#774 让**本包**泛型实例化拿到自己的布局并按布局特化成员，修掉了三种值语义违反。
它显式留下三条未覆盖项。本 change 把它们做完。

三条的现状**实测复核**（树 `wt-geninst` @ `6232c87d1`，全新供种 + 重建 runtime，interp 与
`--mode jit` 逐字相同）：

| # | 形态 | 实测 | C# | 性质 |
|---|---|---|---|---|
| A | `(P2,int) t = (a,7); a.Y = 99` → `t.Item1.Y` | **99** | 2 | 🔴 正确性 |
| A | `(P2,int) t2 = t; t2.Item1.Y = 55` → `t.Item1.Y` | **55** | 2 | 🔴 正确性 |
| B | `List<P2>` 存取后改源变量 | 4 ✅ | 4 | ⚪ **仅密度/分配** |
| C | `GBox<int>` 与 `GBox<string>` 的静态字段计数 | **4**（共享一槽） | 2 / 2 | 🔴 正确性 |
| C | `GBox<int>` 实例 `is GBox<string>` | **true** | False | 🔴 正确性 |
| C | `o as GBox<string>`（o 是 `GBox<int>`） | **放行** → `VCall: expected object, got I64(42)` | null | 🔴 **类型混淆** |

两条对上一轮记录的**修正**（都影响方案选择，故写进 proposal）：

1. **B 不是正确性缺口。** P3a 每元素一个堆 `BoxedStruct`，装箱顺带给了拷贝语义。缺的是
   密度与分配次数，不是值语义。⇒ 它在本 change 里是**性能项**，优先级最低。
2. **跨包元组不需要「泛型体随包投送」。** 上一轮记的是「量级 L，本轮修不了」。实测推翻：
   `Std.ValueTuple2..8` 是**纯 `[Record] struct` 主构造器声明，整个文件 29 行、零方法体**
   （`src/libraries/z42.core/src/ValueTuple.z42`）。阴性对照（同一次编译、同一文件）：

   ```
   本包  [Record] struct Loc<A,B>(A Item1, B Item2)    → struct_alloc Demo.Loc<P2,int> [24B] + struct_fget_prim @8   ✅
   跨包  [Record] struct ValueTuple2<T1,T2>(…)         → obj_new Demo.<unknown> + field_get %14.Item1                ❌
   ```

   两个声明逐字同形，本包那份完全正确。消费方缺的**不是方法体，是许可**——它已持有
   全部字段名/类型（`ExportedClassZ.Fields` → `ImportedSymbolLoader._fillClass`）与全部方法
   签名，合成 record 成员所需的信息一件不缺。

## What Changes

按三个 PR 顺序落，共用本 change 容器与 worktree。

### P1 — 跨包闸门放宽到「成员全可合成」的导入泛型（🔴 正确性，覆盖元组）

闸门今天是一刀切的 `LocalClasses.ContainsKey(inst.Def.Name())`，在两处：
`ExprEmitter.z42:554`（布局/特化）与 `:604`（身份名）。收窄到本包的原始理由写在 550-553：
「消费方只读签名、拿不到生产方方法体」。该理由对**有方法体的泛型**成立，对**零方法体的
`[Record] struct`** 不成立。

改为：定义在本包 **或**（定义是导入的 `[Record] struct` 且其导出成员集恰为编译器可合成集）。
命中后，消费方从导入元数据**反造一个合成 `ClassDecl`** 登记进 `IrGen.GenericDecls`，
其余完全复用 #774 已有的 `IrGenTypeEmitter.EmitInstantiation` 通道。

### P2 — 泛型 class 取独立身份（🔴 正确性，含类型混淆）

三个后果各有独立根因，必须一并修，只修一个会得到自相矛盾的模型：

- `_instClassDesc`（`ClassDescBuilder.z42:507`）合不出完整描述符 ⇒ 不敢给身份
- `_bindIsExpr` / `_bindAsExpr`（`TypeOpTyper.z42:319` / `:46`）**把 `NamedType.Args` 直接扔掉**
- 静态字段键 = `QualifyClass(裸名) + "." + 字段`（`AccessEmitter.z42:348`）⇒ 所有实例化共享一槽

外加两条**解析器缺口**（全仓零先例，`grep` 只命中注释）——不修则 P2 的行为在源码层**无法表达、
无法测试**：

- `GBox<int>.Count`：`ExprParser.z42:114-130` 的泛型出口要求 `<…>` 后紧跟 `(`，`.` 则回滚成二元 `<`
- `(GBox<string>)o`：`ExprParser.z42:365` 的 cast 前瞻是定长 `( Ident )`，`<` 直接落空

> 记忆里记的障碍是「要合成基类链/接口/**vtable**/静态字段」。实测 **vtable 不在 TYPE 段**——
> 它由运行期 `build_type_registry` 从 `own_methods` + 基链 merge 出来。⇒ 负担比记录的小一块。

### P3 — 容器密集化 + 删 P3a 装箱（⚪ 性能）

把**类级**类型实参送到 `List<T>` 内部 `new T[n]` 的分配点。地基已有：`TypeDescCold.type_args`、
`ObjNew` 携 `type_args`、`frame.method_type_args` + `exec_support.rs` 的标记回填。
收益须由 benchmark 说话（分配次数 / RSS / 墙钟），**不达标则不合**。

## Scope（允许改动的文件）

> P1 的 Scope 已精确；P2 / P3 的 Scope 在各自 PR 开工前回到本阶段补精确
> （workflow：实施中发现需改的文件 → 立即停下更新 Scope）。

### P1

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | `ExportedClassZ` 加 `IsRecord`（**不进 ctor 签名**，默认 false、构造后赋值——种子 ABI） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `ecz.IsRecord = (cd.Flags & 8) != 0`（`CLASS_FLAG_RECORD` 已在 TYPE flags，**零格式 bump**） |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | `nct.IsRecord = cl.IsRecord`（`Z42ClassType.IsRecord` 已存在，今天只对本地类回填） |
| `src/compiler/z42c.semantics/src/ImportedGenericSynth.z42` | NEW | 从导入元数据反造合成 `ClassDecl` + 可合成性判据 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | `:554` / `:604` 两处闸门放宽（收敛到单一判据函数） |
| `src/compiler/z42c.semantics/src/IrGenTypeEmitter.z42` | MODIFY | `GenericDecls` 接纳合成 decl；`:68` 的 `rd is Decl` 兜底跟着放宽 |
| `src/runtime/src/metadata/bytecode/class.rs` | MODIFY | 加 `METHOD_FLAG_SYNTHESIZED = 1 << 4`（bit4–7 本就空闲） |
| `src/runtime/src/metadata/lazy_loader/registry.rs` | MODIFY | **D4-fix**：合成实例化产物的重复到达静默跳过，不记歧义 |
| `src/runtime/src/metadata/lazy_loader_tests.rs` | MODIFY | D4-fix 的三情形单测 |
| `src/tests/types/crosspkg_generic_inst_value_semantics.z42` | NEW | A 的 e2e（含 jit 双验） |
| `docs/internals/src/runtime/struct-value-semantics.md` | MODIFY | §收敛面与延后：遗留项 ② 状态更新 + 新闸门判据 |
| `docs/internals/src/compiler/source-compile.md` | MODIFY | 跨包实例化特化的机制记述 |
| `docs/roadmap.md` | MODIFY | 泛型擦除槽那行的「仍未覆盖」三条状态更新 |

**只读引用**（理解上下文必须读，不修改）：

- `src/libraries/z42.core/src/ValueTuple.z42` — 确认零方法体
- `src/compiler/z42c.semantics/src/StructLayout.z42` — `InstDiffersFromDef` / `InstName`
- `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` — `_instLayoutDesc` / `_instClassDesc`
- `src/libraries/z42.ir/src/ZpkgReader.z42` — 确认消费方无方法体读取 API

## Out of Scope

- **有用户方法体的跨包泛型**（如用户自己写的 `Pair<T>` 带方法，跨包使用）。覆盖它需要
  「可重发的泛型模板随包投送」：agent 查实，zpkg 的 `MODS.func` 段里**确实有**函数体字节，
  但① 编译器侧 `ZpkgReader` 无任何读体 API（`ReadModuleTypes` 还主动 `m.Pos += funcLen` 跳过）；
  ② 更根本的是那些体**已按定义布局烘焙好偏移**，不是可再代换的模板。⇒ 真正需要的是投送 AST/模板，
  量级 L，**另开 change**。本 change 命中不了的跨包实例化一律**退回今天的表示**，逐字不变。
- 泛型**接口**的实例化身份（`IEnumerable<int>` vs `IEnumerable<string>`）。
- 泛型**方法**（非类型）的实例化特化。

## Open Questions

- [x] P1 的「可合成集」判据 → **定稿走 `METHOD_FLAG_SYNTHESIZED`**（`method_flags` 的 bit4–7
      空闲，零格式 bump，两个方向优雅降级）。理由与另两条的否决见 design.md §D1。
- [x] 🔴 **合成实例化产物跨模块重复** → 已实测定性为 **P1 的先决条件**，设计已修正。
      见 design.md §D4：合成 ctor 同名重复会让**调用即抛**，且「库内部用了元组、主程序也用了」
      就已撞上。修法 = 加载器区分合成产物与用户声明。
- [ ] P2 的静态字段换键走**分阶段引入**（User 已裁决）：support 先行、晚一个 nightly 再 use。
      具体分几步、过渡期两种键怎么共存 → P2 开工前在 design.md 定稿。
- [ ] P3 的达标线（分配次数 / RSS / 墙钟各降多少才算值得）→ P3 开工前定。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
