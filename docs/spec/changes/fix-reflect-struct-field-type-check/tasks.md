# tasks: fix-reflect-struct-field-type-check

> 类型：**fix**（反射往 struct 字段写值时把 struct **半写坏**）｜ 创建：2026-09-28
> 出身：审计 R3 的 ③（缺席）分类过程中，探针意外撞出的两处**写侧**可达缺陷。
> 前序同族：**#906**（基元字段 ← Null 静默无效）· **#908**（数组元素 ← 类型不符静默清零）。

## Why（实测）

```z42
class Holder { public int n; public Point pt; }   // struct Point { int x; int y; }
h.pt.x = 11; h.pt.y = 22;
field(typeof(Holder), "pt").SetValue(h, 42);      // 42 作为 object 传参
```

| | 修前 | 修后 |
|---|---|---|
| `SetValue(h, 42)` → `pt` | 🔴 **静默通过**，`pt` 变成 **(42,22)** | 抛，`pt` 保持 (11,22) |
| `SetValue(h, "hello")` → `pt` | 抛 | 抛（不变） |
| `SetValue(h, 装箱 Point)` | 通过 (99,88) | **通过 (99,88)**（回归门） |

**兄弟路径逐字同形**：装箱 struct 的嵌套 struct 叶子（`struct Line { Point a; }` 的 `a`）
`a=(11,22)` ⇒ **(42,22)**。

⇒ 不是丢弃写入，是**数据损坏**：得到一个**半写坏的 struct**（只有第一个叶子被改），零报错。

## 根因：谓词回答的是另一个问题

守卫是

```rust
let src = match value {
    Value::BoxedStruct(s) => s,
    other => bail!("… expected a boxed struct, got {other:?}"),
};
```

它只问「**是不是**一个装箱 struct」。而 `SetValue(obj, 42)` 里的 `42` 作为 `object` 形参传入时
**会被装箱成 `Std.Int32`** —— 那是个**合法的 `BoxedStruct`** ⇒ 通过该门，接着代码按字段区域
宽度 `size` 把它的字节拷进对象的 struct 区域 ⇒ 4 字节落在 struct 的第一个叶子上。

`"hello"` 之所以被拦住，是因为它是 `Value::Str`、压根不是 `BoxedStruct` —— **那道门拦住的
恰好是最不像的那格，放过了最像的那格。**

⭐ 与 **#892 逐字同构**：*「校验过了」要问清那个谓词回答的是哪个问题* ——
「它是装箱 struct 吗」与「它是一个 `Point` 吗」不是同一件事。
（#892 那次是「这个键在表里吗」与「有一个真叫这个名字的函数吗」。）

## What Changes

两处守卫之后各加一条**类型**判据：

- `object_inline_struct_field_set`（对象的内联 struct 字段）：期望类型来自
  `struct_field_fq(&resolve, &class_name, name)`（该函数已在上游调用、返 FQ 名）。
- `boxed_struct_field_set` 的 `leaf.is_struct` 分支（装箱 struct 的嵌套叶子）：
  `FieldLeaf` **只有 `is_struct`、不带类型名**，故用同一个 `struct_field_fq` 现取。

⚠️ **判据是全限定名逐字相等，不做短名回退** —— 短名回退会让 `a.Point` 冒充 `b.Point`，
那是把一个静默错值换成另一个（同款教训见 `Z42InterfaceType.SameInterface` 的注释）。

## Tasks

- [x] 两处守卫收紧为「是这个 struct 类型」
- [x] e2e fixture `src/tests/reflection/struct_field_type_check` + `opt_all`（interp / jit 双档一致）
- [x] **回归门**：写真 `Point` 照旧成功（两条路各一格）+ 基元字段照旧
- [x] **阴性对照**：把两条判据改成恒假后重建 release VM，fixture 第一格即 `no throw` +
      `Assert.Equal(11, h.pt.x)` 失败（`pt.x` 被半写成 42）
- [x] `cargo test --lib`（不带过滤）1375/0 · 4 个 feature 组合（interp-only/ios/android/wasm）全过
- [x] `xtask test e2e` 748/87/3 · `test stdlib` 347 组 · `test compiler` 24/24 + 不动点 3/3 · `test docs` 零新增
- [x] 文档：`docs/reference/src/stdlib/reflection.md` 补「值 struct 字段只接受自己类型的装箱值」+ 历史
- [ ] GREEN：CI 全矩阵绿

## 不做（Out of Scope）

- **不动读侧 `field_value`**。它把**五种**含义压成 `Value::Null`（槽越界 / ref 槽越界 /
  `TAG_UNKNOWN` struct 根 / `decode_prim` 失败 / 真的是 null），是 R3 ③ 的正主；
  但实测**反射入口先按名字在目标自身类型上定槽**（找不到就 bail）⇒ 越界那几格**不可达**。
  没有已证实的可达缺陷，不在本刀凭判断改。
- **不做 ④ 的另一半 —— 而且实测之后判定它*不该*做。** 分类时量出 **11 处**
  `ret.unwrap_or(Value::Null)`：`void` **z42 函数**的返回值同样被表示成 Null
  （#912 只做了 builtin 那一半）。**测了可达性，结论是「没有可达缺陷」**：

  | 测点 | 结果 |
  |---|---|
  | `voidMethod.Invoke(obj, …)` 经反射 | 返回 **`null`** —— **这正是 C# 的行为**（`MethodInfo.Invoke` 对 void 方法返回 null），是文档化契约、属含义 ① **正当** |
  | 内部 `frame.set(dst, Null)` | dst 在良构 IR 里不会被读；编译器也不允许把 void 调用的结果赋值 ⇒ 无可观测后果 |

  ⇒ 与 builtin 那一半**不同**：#912 有一处真实误用被 `exec_builtin_value` 当场抓到
  （`__println` 的结果被当值用）；函数这一半找不到对应的东西。
  改它要动 `Frame::ret` 的表示、跨 interp + jit 11 处，而**边界上仍然得返回 null**
  —— 纯支出。**故不做**，而不是「留待以后」。
- **不碰 ⑥**（22 处 `args.get(N).unwrap_or(..)`）：被 arity 门阻塞，那道门需独立设计。
