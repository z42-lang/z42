# Proposal: 嵌套泛型反射参数（nested generic GetGenericArguments）

## Why

反射 MVP（0.3.12 C 主线「反射完整化」）**只剩这一项未落地**：嵌套泛型
`typeof(Box<Pair<int,string>>)` 的类型实参 `Pair<int,string>`，在 z42c 发射 `Typeof`
指令时被 [ExprEmitter.z42](../../../../src/compiler/z42c.semantics/src/ExprEmitter.z42) 的
`_typeofName` 压成裸定义名 `"Pair"`——内层 `<int,string>` **在编译期即丢失**。于是
`typeof(Box<Pair<int,string>>).GetGenericArguments()[0].GetGenericArguments()` 返空、
`IsGenericTypeDefinition` 恒 true，无法反射嵌套构造泛型。

根因在**产出端**（emitter 扁平化）。不做 → 反射 0.3.12 C 主线无法宣布完成；嵌套泛型的类型
内省（序列化框架、泛型容器反射）不可用。

## 方案抉择（A，2026-07-23 User 二次裁决）

- **方案 B（结构化递归 wire）**：`Typeof` opcode 携递归 `TypeNode` 树。**否决**——它改
  `TypeofInstr` 的 z42c↔z42.ir 接口 + 触发 zbc/zpkg 格式 bump，撞自举纪律（bootstrap-seed.md
  axis ③/④：z42c 新用一个 z42.ir API 要晚一个 nightly；CI 两代自举 gen1 z42c 编不过因种子
  z42.ir 无新 API——实测本地复现 E0401）。
- **方案 A（发括号实参串 + runtime 递归解析）**：**采纳**。`_typeofArgName` 递归产带尖括号的
  完整实参名 `Pair<int,string>`，塞进 `TypeofInstr` 现有 `string[]` 槽——**z42c↔z42.ir 接口
  不变、无格式 bump**；runtime `make_type_from_name` 检测 `<...>` 按括号深度递归解析 →
  `make_constructed_type`（逐 arg 再回 `make_type_from_name` → 天然递归）。改动只落 z42c
  emitter（一函数）+ Rust runtime（reflection.rs），单变更干净落地，不碰自举/CI。

## What Changes

- z42c `_emitTypeof` 的**实参**发射改用新 `_typeofArgName`（instantiated 递归产带尖括号完整
  名；顶层与嵌套同款）。`TypeofInstr` 仍携 `string[]` 实参、wire 布局不变。
- 运行期 `make_type_from_name` 识别 `<...>` → `split_generic_args`（括号深度感知）拆 base +
  顶层 args → `make_constructed_type` 递归构造嵌套 `Std.Type`（挂 `__typeArgs`）。
- 顺带修 [version-bumping.md](../../../../.claude/rules/version-bumping.md) **陈旧路径**（`z42c.ir`/`z42c.project`
  → 收敛后 `z42.ir`）——独立规范冲突修正（User 批准），与 A 机制无关、不 bump 版本。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | 新增递归 `_typeofArgName`；`_emitTypeof` 实参改用之 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `make_type_from_name` 加 `<...>` 括号解析 + 新增 `split_generic_args` |
| `src/tests/types/nested_generic_args.z42` | NEW | e2e：嵌套构造泛型 GetGenericArguments（覆盖 spec 全场景） |
| `docs/design/language/reflection.md` | MODIFY | `reflection-future-nested-generic-args` 标记已落地（方案 A）+ 构造型泛型节 |
| `docs/roadmap.md` | MODIFY | 0.3.12 C 主线「嵌套泛型 args」→ ✅（方案 A，无 bump） |
| `.claude/rules/version-bumping.md` | MODIFY | 修陈旧路径（z42c.ir/z42c.project → z42.ir，User 批准的独立冲突修正） |

**只读引用：**

- `src/compiler/z42c.semantics/src/Z42Type.z42` — `Z42InstantiatedType.TypeArgs` 树结构
- `src/libraries/z42.ir/src/IrInstr.z42` — `TypeofInstr`（确认 `string[]` 接口不动）
- `docs/spec/archive/2026-06-16-add-reflection-generic-type-definition/` — 前身（顶层结构化 wire）

## Out of Scope

- 泛型方法 `Method.Invoke` / `MakeGenericType` / `Activator.CreateInstance<T>`（0.4.x G 流）
- enum 反射（需 enum 类型实体设计）
- 实例路径嵌套（`new Box<Pair<int,string>>()` 的 `obj.GetType()`）——复用同一 runtime 解析，
  若 `ObjNew` 实参串带括号则同样递归；本变更只验 `typeof` 形式
- 泛型实参内嵌数组 `typeof(Box<int[]>)`（同 jagged 数组，type 解析器不接受嵌套 `[]`）

## Open Questions

- 无（B-vs-A 抉择已由 User 二次裁决为 A；version-bumping.md 陈旧路径修正已获批准）
