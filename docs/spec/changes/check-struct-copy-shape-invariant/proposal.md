# `StructCopy` 的「同类型」不变式从静默 `min` 改为校验（并把 D-2 P1 结案）

> 类型：**fix（vm 类）**｜ 创建：2026-09-28｜ 状态：**IMPL**
> 出身：`symbolic-struct-field-access`（审计 D-2）的 **P1**。动手前按惯例先读代码核对 P1 的定义，
> 结果是 **P1 两半都已经做完了**（而提案没记），同时露出一处 R3-⑤ 形态的静默吞。本刀做后者。

## 0. 先更正提案：D-2 的 P1 已经不存在了

`symbolic-struct-field-access` 的 P1 写着两件事，且阶段表标「同 P0 一次 **bump**」：

| P1 原计划 | 事实 | 依据 |
|---|---|---|
| `StructAllocInstr.Size` 降级为纯兜底 | ✅ **早已如此** | `exec_struct.rs::resolve_layout` 先查 `TypeDesc.struct_layout()`，**只在拿不到时**才用编码里的 `size` 造一个 size-only 布局 |
| `StructCopyInstr` 补 `TypeName`（让它能解析布局） | ✅ **不需要** —— 它**已经**在用布局 | `StructArena::copy_into(.., _size: usize)` —— 那个参数**下划线前缀、完全没用**；它拷的是 `min(src.bytes.len(), dst.bytes.len())`，即两个 blob **各自的布局** |

两个 blob 都是 arena 槽、**各自带 `layout: Arc<StructTypeLayout>`**，所以 `StructCopy`
从来不需要指令里再带类型名。⇒ **P1 不需要任何格式 bump，也没有剩余工作。**

> 🔴 提案内部本来就自相矛盾：§「下一步」把「才需要格式 bump」挂在 P2 上，而阶段表写 P1
> 「同 P0 一次 bump」。用事实判：`StructAlloc` 编码**已含** `typeName(u32 池)`，
> `StructCopy` 编码是 `src(u16) + size(u32)`。矛盾按「事实」一侧解决并回写提案。

## 1. 但这块地上有一处静默吞（R3-⑤ 形态）

`copy_into` 的文档写着不变式「both pre-allocated, **same type**」，而代码只用 `min` 兜：

```rust
let n = src.bytes.len().min(dst.bytes.len());
dst.bytes[..n].copy_from_slice(&src.bytes[..n]);
let rn = src.refs.len().min(dst.refs.len());
dst.refs[..rn].clone_from_slice(&src.refs[..rn]);
```

`min` 保证**内存安全**，但它同时保证**不变式被破坏时没人知道** —— 结果是一个
**静默截断 / 只拷了一半的 struct**，而腐坏会在离现场很远的地方现形。这正是
`fix-silent-prim-field-write` / `fix-silent-array-elem-zero` /
`fix-reflect-struct-field-type-check` 三刀修的同一形态。

**两个见证都在手边，而都被丢掉了**：

1. **两个布局本身** —— 同类型 ⇒ `src.bytes.len() == dst.bytes.len()` 且引用槽数相等，无条件成立。
2. **编译器的 `size`** —— `StructCopy` 携带的是**编译期**为这个类型算出的大小。它以 `_size`
   到达此处后被**直接扔掉**。它是一次免费的对账：「编译器对这个 struct 大小的认识，
   和运行期解析出的布局一致吗？」而这正是 `z42-generic-instantiation-layout` 那条线反复撞到的偏斜
   —— 在那里它以 `struct field write out of blob bounds` 崩出来，但**只是因为恰好有一次写跑出了尾端**。

## 2. 做法（政策照既有的抄）

`StructArena::check_copy_invariant(src, dst, size)`：

- `src.bytes.len() != dst.bytes.len()` 或引用槽数不等 ⇒ 形状不同，`bail!` 并**打出两个类型名与各自尺寸**
- `size != dst.bytes.len()` ⇒ 编译器与运行期布局对这个类型的大小不一致，`bail!`

**`debug` `bail!` / `release` 放行** —— 照 `__box_prim`（#837）与 `prim_value_mismatch`（#908）
已定的政策：**用户写不出能走到布局偏斜的 z42**，只有编译器或加载器的 bug 能，
所以 release 不该对用户抛。release 保持原样（`min` 拷贝），零行为变化、零开销。

## 3. 验证：这道门只在 debug 存在 ⇒ `xtask test`（release）证明不了它

⚠️ 这是本刀最容易走过场的一格。`cfg(debug_assertions)` 的门在 release VM 里**根本不存在**，
所以 `xtask test` 全绿对它**一个字都没说**。必须让**真实编译出来的 `StructCopy`** 跑在 **debug VM** 上。

配方（无需两代自举、无需 xtask）：

```bash
export Z42_LIBS="$PWD/artifacts/build/libraries/dist/release"
VM_R=$PWD/artifacts/build/runtime/release/z42vm     # 编译用（快）
VM_D=$PWD/artifacts/build/runtime/debug/z42vm       # 跑用（门已武装）
Z42C=$PWD/artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg
"$VM_R" "$Z42C" -- --emit-zbc <case>.z42 out.zbc --opt-all
"$VM_D" out.zbc Main        # 裸 zbc 也要 Z42_LIBS，否则假崩成 VCall 类错误
```

> ⚠️ 覆盖面必须是**全类目**而不是 `src/tests/{structs,types}` —— `StructCopy` 由
> 「任何按值赋值 / 传参 / 返回」发射，泛型类目尤其重要（偏斜正是从那里来的）。

## 4. 顺带修掉的文档漂移

`docs/internals/src/runtime/struct-value-semantics.md` 写「字段 byte offset / **size** 由编译期
烘焙为**立即数**，运行时**无需查表**」。对 `byte_off` 成立；对 `size` **已经不成立**
（`StructAlloc` 查布局、`size` 只兜底；`StructCopy` 的 `size` 完全不用）。这正是
「P1 已做完却没人记下来」的文档面 —— 而它同时让 D-2 P2 的读者以为 size 也要一起符号化。
