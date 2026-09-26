# tasks: make-silent-fallbacks-signal

> 类型：**fix**（把静默兜底变成 debug 信号；release 行为不变）｜ 创建：2026-09-27
> 出身：[结构审计 2026-09](../../../internals/src/runtime/vm-architecture.md) 的 R3-b，
> 外加 U-3 的**降级处置**（实测判定不是 bug，改为把前置条件的理由写下来）。

## Why

审计 R3 那一族的共同形状是「热路径上的兜底把元数据错配翻译成貌似合理的值」。其中**最坏的一档
不是 Null，是 0**：

```rust
ArrayBacking::I32 { .. } => { s[i] = if let Value::I64(n) = val { n as i32 } else { 0 }; }
```

往 `int[]` / `byte[]` / `char[]` / `double[]` / `long[]` / `bool[]` 的 backing 写一个类型不符的
`Value`，**debug 与 release 都不响**，直接存 `0` / `'\0'` / `false` / `0.0`。

而**同一个 `match` 里的两个 `StructBytes` 臂是带 `debug_assert!(false)` 的** —— 判据在一个函数
内部就不一致。

为什么比同族的「读不到就给 `Null`」更坏：`Null` 至少是个可疑值、会在下游某处炸出来；
`0` 是程序**完全无法与合法写入区分**的答案 —— `int[]` 里的一个 `0` 既可能是用户写的，
也可能是这里吞掉的一次类型错误。

同一形状在 `pack_backing`（`ArrayNewLit` 打包数组字面量）里还有一份，六个臂同病。

## What Changes

新增 `prim_value_mismatch(val, backing, site)`（`array.rs` 模块级，判据只有一份），
两处 **12 个回落**全部先过它：

- `array_access.rs` 的 `set_boxed` —— 6 个基元臂
- `array.rs` 的 `pack_backing` —— 6 个基元臂

**取 `debug_assert!` 而不是 `bail!`**，判据同 `__box_prim` 收到 `Null` 那条先例：

- golden 语料默认跑 debug VM ⇒ **CI 本身就是探测器**；
- 「类型不符」是编译器该在 `ArraySet` / `ArrayNewLit` 站点转换/拆箱掉的事，**不是用户的错**
  ⇒ release 放行、保持今天的行为（不拿用户崩溃换诊断能力）；
- 若若干版本一直不响，再按 `gc/refs.rs:416-437` 那条先例考虑提升为无条件 `assert!`。

## 顺带：U-3 的降级处置（不是 bug，但注释欠一个理由）

审计把 `synthesize_object_layout` 列为「已与编译器不一致」，**实测推翻**：

- zbc writer 的对象布局块 gate 是 `(cd.Flags & 116) == 0` = struct｜interface｜enum｜delegate
  ⇒ **每个普通 class 一律带布局块**，走不到那个兜底；
- 格式 strict-pin ⇒ 「旧 minor 没有布局块」这条路不存在；
- 泛型 class 的实例化描述符也填了对象布局块（`ClassDescBuilder.GenericInst._instClassDesc`）。

所以它的注释里那句「a struct field never occurs in a layout-less type」**成立**，只是此前
**只断言、没给理由**。而这里**加不了断言**：该函数只拿到 `&[FieldSlot]`、没有类型注册表，
`tag_from_type_name` 对任何非基元名都给 `TAG_OBJECT`，无从区分 struct 与 class。
⇒ 处置是把「为什么成立」与「要加检查该加在哪（写端或调用方）」写进注释。

## Scope（允许改动的文件）

- `src/runtime/src/metadata/types/array.rs`
- `src/runtime/src/metadata/types/array_access.rs`
- `src/runtime/src/metadata/types/layout.rs`（仅注释）

## Tasks

- [x] `prim_value_mismatch` 单一判据 + 两处 12 个回落接上
- [x] `layout.rs` 的前置条件补理由（含「要加检查该加在哪」）
- [x] debug 构建通过
- [x] `cargo test --locked --workspace --features z42-test-fixtures --lib`（**debug**）：
      **1367 passed / 0 failed**，断言零响
      > ⚠️ 我第一次跑漏了 `--workspace`，包选择落到 default-members ⇒ **只跑了 21 个就报绿**。
      > 源码注释里明确警告过这一点（`fix-more-silent-gates` 修过一次）。权威调用在
      > `xtask_test.z42` 的 `_testRuntimeUnits`，照抄它。
- [ ] `xtask test e2e`（跑 **debug** VM，真正的探测器）：断言零响
- [ ] GREEN：全量；CI 全矩阵绿

## 不做（Out of Scope）

- **不提升为无条件 `assert!`**。先让它在 debug 下跑若干版本；这也是 `__box_prim` 那条的走法。
- **不动 `get_boxed` 的读侧**（那侧的 `StructBytes` 臂已有断言，其余臂读的是自己写进去的字节）。
- **不改 `field_get` 的 `Value::Null` 兜底**（审计 R3-a）。那条要给「缺值」一个独立的 `Value`
  变体，是需规范先行的中刀，单独立项。

## 验证

- release 行为**逐字不变**（`debug_assert!` 在 release 下编译掉）⇒ 无格式 bump、无指纹 bump、
  产物字节不变。
- 判别力：断言的对象是「编译器本不该送进来的输入」，**当前语料里不该有** ⇒ 零响即预期。
  若将来响了，那就是抓到了一次真的静默错值（这正是它存在的意义）。
