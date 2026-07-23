# Design: 嵌套泛型反射参数（方案 A — 括号实参串 + runtime 递归解析）

## Architecture

```
z42c 编译期                        TypeofInstr (wire 不变)      z42vm 运行期
─────────────                      ────────────────────         ────────────
Z42InstantiatedType (已是树)
  Def=Box, TypeArgs=[Pair<int,str>]
       │ _emitTypeof（顶层 args 各走 _typeofArgName）
       ▼
TypeName="Box"                     Op.Typeof                    Instruction::Typeof
TypeArgs=["Pair<int,string>"]      dst | name:u32 | count:u8    TypeofInsn{type_name,
  （_typeofArgName 递归产带尖括号   | [argStr:u32]×count           type_args:Box<[String]>}
    的完整实参名——顶层 & 嵌套同款）  （string[] 布局不变）             │ make_constructed_type(name, args)
                                                                    ▼ 逐 arg → make_type_from_name
                                                                 检测 '<' → split_generic_args
                                                                 → make_constructed_type（递归）
                                                                 嵌套构造 Std.Type + __typeArgs
```

**递归的落点在 runtime**：`make_constructed_type` 逐 arg 调 `make_type_from_name`，后者遇 `<...>`
再拆再构造——单一入口天然递归到任意深度，无需任何新数据结构 / 新 opcode / 格式改动。

## Decisions

### Decision 1: 方案 A（括号实参串 + runtime 解析）而非 B（结构化递归 wire）

**问题：** 嵌套实参如何承载？（初次裁决选 B；实施中发现 B 撞自举纪律，User 二次裁决改 A。）

**选项：**
- **B（结构化递归 wire）**：`Typeof` opcode 携递归 `TypeNode` 树。忠于 generic-type-definition
  Decision 1 的结构化方向。**致命代价**：改 `TypeofInstr` 的 z42c↔z42.ir 接口（新 `TypeNode`
  类 + 构造签名 `string[]`→`TypeNode[]`）+ zbc/zpkg 格式 bump。z42c.semantics 消费该 API →
  自举时 gen1 z42c 对着**种子 z42.ir**（无 `TypeNode`）编 → `E0401`（bootstrap-seed.md axis
  ③/④；本地两代自举实测复现）。要么两阶段跨两个 nightly，要么改 `ci-bootstrap` 两代自举先
  重建 z42.ir——均超本变更 Scope、且后者本地实测又暴露 z42.ir 导出层连锁问题。
- **A（括号实参串 + runtime 解析）**：z42c 只改 emitter 发**带括号的实参串**塞进现有 `string[]`；
  runtime（Rust，无自举约束）解析括号。**z42c↔z42.ir 接口不变、无格式 bump、不碰自举/CI**。

**决定：** 选 **A**（User 2026-07-23 二次裁决）。pre-1.0 下，B 的结构化-wire 纯度不值多 nightly /
CI 手术的代价；A 的嵌套括号串匹配无歧义、非启发式（philosophy 反对的是「拿 sentinel 猜」，非
「解析一个良构串」），且**顶层实参仍走结构化 `string[]` 槽**，仅嵌套层用括号串。

### Decision 2: 顶层名 vs 实参名分两个 helper

`_emitTypeof` 的**根名**仍用 `_typeofName`（instantiated → 裸 `QualifyClass(def)` = `"Box"`，
顶层 args 另发 `string[]`）——不变。仅**实参**改用新 `_typeofArgName`：instantiated → 带尖括号
完整名（递归）；其余复用 `_typeofName` 叶子名。理由：根的构造信息由 `TypeName + string[] args`
表达（既有），实参才需要「自描述」的完整名以便 runtime 重建其内层 args。

### Decision 3: runtime 递归自然落在 make_type_from_name

`make_constructed_type`（既有，`&[String]`）逐 arg 调 `make_type_from_name`。只需让后者识别
`<...>`：`split_generic_args`（括号深度感知，`Box<int>,string` → `["Box<int>","string"]`）拆
base+顶层 args → `make_constructed_type`。逐 arg 再回 `make_type_from_name` → 天然递归。
**无需新增 runtime 数据结构**（不引 `TypeNode` struct）。

## Implementation Notes

- **z42c** `ExprEmitter.z42`：新增 `private string _typeofArgName(Z42Type t)`——`Z42InstantiatedType`
  → `QualifyClass(Def.Name()) + "<" + 各 arg 递归 join(",") + ">"`；否则 `_typeofName(t)`。
  `_emitTypeof` 实参循环 `_typeofName` → `_typeofArgName`。根名不动。
- **runtime** `reflection.rs`：`make_type_from_name` 在数组 `[]` 判定后、类型注册表查找**前**插入
  `if name.find('<')..ends_with('>')` → `split_generic_args(inner)` → `make_constructed_type`。
  新增 `fn split_generic_args(&str) -> Vec<String>`（`<`/`[` 增、`>`/`]` 减深度，顶层 `,` 切）。
- **无格式 bump**：`TypeofInstr`（z42.ir）、`TypeofInsn`（Rust）、zbc/zpkg 版本常量**全不动**。
- **无自举影响**：z42c 源不新用任何 z42.ir API；no new syntax → 上一 nightly z42c 直接能编。

## Testing Strategy

- **e2e golden**（`src/tests/types/nested_generic_args.z42`，Assert 式）：`Box<T>`/`Pair<A,B>`，
  覆盖 spec 全 Scenario（一层 / 多层嵌套 / 平铺不回归 / 嵌套 Name / interp+jit）。
- **平铺不回归**：复用 `generic_type_definition.z42` / `instance_generic_args.z42` /
  `generic_predicates.z42`（须仍全绿）。
- **z42c 自举**：`xtask test compiler` — emitter 改动不破坏 gen1==gen2 byte-identical。
- **完整 GREEN**：`xtask test`（全 stage）。因无格式 bump，本地 warm 路径即可全验（无需 CI 两代）。
