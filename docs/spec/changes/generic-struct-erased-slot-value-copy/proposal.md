# Proposal: 泛型擦除槽的值复制（generic-struct-erased-slot-value-copy）

> Status: **DRAFT —— 等 User 确认**（方向已于 2026-09-22 裁决：存入即复制 + 外层深拷，对齐 C#；
> 本文档待裁的是下面「§待裁决」那一条：拷贝的**驻留位置**）
> 分类：vm（执行语义变更）+ lang（值语义规则）→ **规范先行**；子系统：compiler 前端 + runtime
> 承接 [fix-generic-struct-chain-access](../../archive/2026-09-15-fix-generic-struct-chain-access/)（修链式偏移时实测发现本缺陷并登记 Deferred）

## Why

struct 值存进**声明类型为型参 `T` 的字段**（泛型 struct 与泛型 class 都是）时**按句柄存、不复制**；
复制外层泛型 struct 也只浅拷该句柄。2026-09-22 在 main `134c7d962` 实测，三种形态全部违反值语义：

| # | 形态 | z42 实测 | C# 应为 |
|---|---|---|---|
| ① | `P2 inner; var pp = new Pair<P2,int>(inner,1); inner.Y = 99;` → `pp.First.Y` | **99** | 2 |
| ② | `P2 c; var cb = new CBox<P2>(c); c.Y = 77;` → `cb.Item.Y` | **77** | 2 |
| ③ | `var q = p3; q.First.Y = 9;` → `p3.First.Y` | **9** | 2 |

### ⚠️ 形态 ② 不只是错值，是 use-after-free

`new CBox<P2>(c)`（泛型 **class**）把一个**栈帧作用域的 arena `StructRef`** 存进了 GC 堆对象的字段槽
（`AccessEmitter.z42:140` → `FieldSetInstr`，不经任何复制/装箱）。ctor 帧一弹，arena 即按 LIFO 截断
（`struct_arena.rs:71-75`），`cb.Item` 成**悬垂句柄**。实测看到的 `77` 是它最温和的表现；
中间隔一次会推进 arena 的调用，就是真的读已释放槽位。

**⇒ 本 change 是健全性修复，不只是与 C# 的语义对齐。** ctor 实参那一刀应最先、可独立落地。

## 根因

泛型 struct 声明为 `T` 的字段按**定义**布局：`StructLayout._kindOf("A")` 认不出 `A`，落到
`StructLeafKind.GcRef`（`StructLayout.z42:498-505`）⇒ 8 字节**引用叶子**；`Tag.FromName("A")` →
`Tag.Object`（`ZbcFormat.z42:86-103`，一切不认识的名字都归 `Object`）。于是：

- 每次存入 = 引用叶子写一个裸 `Value`；
- 每次外层复制 = `refs.clone_from_slice`（`struct_arena.rs:137-159`）= 句柄 `clone()`。

内联（blob）字段两条机制都正确深拷；**引用叶子按设计就是浅拷**——这是 D1-a 不变式，不能改。

## 六条写入路径的现状

| # | 形态 | site | 复制？ |
|---|---|---|---|
| A | `new Pair<P2,int>(inner,1)` 调用点 | `CallEmitter.z42:410-412`（`_emitNew`）| ❌ **完全没有 copy-in** |
| B | ctor 体 `this.First = a`（泛型 struct）| `AccessEmitter.z42:290-302`，`FieldIsStruct` 为假 | ❌ |
| B′ | 裸 `First = a` | `AccessEmitter.z42:252-256` | ❌ |
| C | `pp.First = inner` | 同 B | ❌ |
| D | ctor 体 `this.Item = c`（泛型 class）| `AccessEmitter.z42:140` | ❌ **+ 堆逃逸** |
| E | `arr[i] = pairValue` | `AccessEmitter.z42:515-530` `_copyRegion` | ❌ 同上 |
| F | `Holder.P = v`（静态 struct 字段）| `_boxIfStaticStruct`（`:388-394`）| ✅ 已装箱 |

两个结构性发现：

1. **`_emitNew` 从不 copy-in**。普通 `BoundCall` 的 struct 实参走 `_emitStructAwareArgs`
   （`CallEmitter.z42:17-32`：逐个 `StructAlloc` + `StructCopy`），`_emitNew`（`:398`）**从不调它**。
   非泛型 struct 之所以没暴露，是 ctor 体里 `this.From = f` 命中 `FieldIsStruct` 走了 `_copyRegion`
   ——擦除槽只是把这层遮蔽掀开。
2. **ctor 实参从不过 `BoxArgs`**。`TypeChecker.BoxArgs`（`:261-272`）唯一汇聚点是
   `OverloadBinder._withDefaults`（`:243`）；`ConstructTyper._bindCtorArgs`（`:196-340`）
   零个 `BoxIfNeeded` 调用——尽管 P3a 提案文本声称覆盖「`ctor` 等」。
   这已是同一教训的**第四次**复发（`struct-value-semantics.md:498-501`：
   「任何手搭 `BoundCall` 而不经 `_withDefaults` 的路径都会漏装箱」）。

## 关键约束：必须有运行期的一半

z42 泛型是**类型擦除**的——`Pair.Pair$2` 只有一份函数体、一个裸名 `TypeDesc`。ctor 体内 `a` 的静态
类型就是 `Z42GenericParamType A`，**编译器无从知道它是不是 struct**；外层 `_copyRegion` /
`StructCopy` 面对的叶子声明类型同样是 `A`。

⇒ **纯前端修不完**，路径 B/B′ 与外层复制必须在运行期判别。

**零格式变更的判别子**：擦除槽在坏路径上拿到的是**裸 `Value::StructRef`**，而 `object` / 接口叶子只
可能是 `BoxedStruct`（`BoxIfNeeded` 在那条边界上装箱，`TypeChecker.z42:239`，且装箱必须保持引用身份
P4b）。「引用叶子收到裸 `StructRef` → 取值复制」自描述、不会误伤 `object` 叶子。

> 被否的替代：给布局加 `StructLeafKind.Erased`。它改动 zbc TYPE 段的 `ref_kinds` 字节
> ⇒ 格式 bump + version-bumping 清单 + bootstrap-seed 分阶段纪律。`ref_kinds` 今天没有非测试消费者，
> 读端便宜，但**流程代价不便宜**，且会撞 `cross-zpkg/struct_nested_layout_cross_pkg` 的逐字节一致要求。

## §待裁决（唯一的设计岔路）

**拷贝驻留在哪？** 擦除槽的属主有四种：

| 属主 | 正确驻留 |
|---|---|
| (a) 栈帧 blob（`Pair<P2,int>` 局部）| arena |
| (b) 泛型 class 的堆对象字段（`CBox<P2>`）| **必须堆** |
| (c) `struct[]` 元素 | **必须堆** |
| (d) `BoxedStruct` 内 | **必须堆** |

- **选项 1：一律装箱**（`__box_struct`）。统一、直接复用 P3a 的 `as_cast` `StructRef` 恒等臂；
  代价是 (a) 也付一次堆分配，且**读路径要一并改**（见下「读路径风险」）。
- **选项 2：按属主分流**（(a) 走 arena 复制，(b)(c)(d) 装箱）。(a) 零堆分配，但两条码路要各自维护，
  且属主判定本身要在运行期做。

我的建议：**选项 1（一律装箱）**——(a) 这一路的分配可以后续单独优化，而两条码路的维护成本
和漏判风险是立刻就要付的；且 P3a 已经把装箱/拆箱那套跑通了，复用面最大。

## 读路径风险（装箱后必须同批处理）

今天 `var f = pp.First;` 降成 `StructAlloc` + `StructCopy(f, 槽里的原始值)`（`StmtEmitter.z42:36-50`），
而 `struct_copy_val` 的 `as_struct_ref`（`exec_struct.rs:370-375`）**硬拒一切非 `StructRef`**
→ `StructCopy src: expected a struct value (StructRef), got BoxedStruct`。字段读今天**不拆箱**
（`StructUnboxTarget` 只在方法/索引器返回处触发，`MemberResolver.z42:182` / `ExprTyper.z42:177`，
从不作用于 `BoundMember`）。两条路子：
(i) `_emitBlobFieldGet`（`:269-286`）在「槽擦除 + 代换后是 blob struct」时发 `AsCast`——复用 P3a 的
恒等臂，两种运行期形态都吃；(ii) 放宽 `struct_copy_val` 接受 `BoxedStruct` 源。(i) 更合 P3a 的路子，
(ii) 改动更小。

⚠️ 同时必须保住 `_structChainRoot`（`:411-430`）的行为：它**刻意不拆箱**静态字段根（`:419-422`），
好让叶子写能落回去。擦除槽的链根必须同款——**链根取原始箱，值读才取副本**。否则
`generic_struct_chain.z42` 的 `pp.First.Second = 40L` 会静默写进一个副本、改动凭空消失。

## 回归面（按风险降序）

1. **`Std.ValueTuple2..8` 的字段全是型参**（`ValueTuple.z42:18-29`）⇒ 每个槽都是擦除槽，
   record 主 ctor 展开成 `this.ItemN = ItemN` 即路径 B。**存入侧的改动会在每一次元组构造上触发，
   包括 z42c 编译它自己**。故判据必须是「**传入值是 struct**」而非「叶子是擦除的」，
   否则每个 `(int,string)` 付一次装箱、自举字节必漂。
2. `src/tests/types/generic_struct_chain.z42` —— 直接对手，`:52`/`:62-64`/`:73-75` 全靠写穿存活。
   `:73-75` 最尖：擦除槽在 **`struct[]` 元素**内，副本必须堆驻留，否则拿别名换悬垂。
3. **自举字节不动点**（`xtask test bootstrap` 要 gen1 == gen2）：前端改动会动指令流，
   `_emitNew` 的 copy-in 尤其会在今天光秃秃的调用点加出 `StructAlloc`+`StructCopy` 对。
4. `struct_generic_container.z42`（P3a golden，已走箱→拆箱；注意 `builtin_box_struct:89` 对已装箱输入
   是幂等的，靠它防双重装箱——要实证这条臂真的被走到）、`generic_struct_equals.z42`、
   `cross-zpkg/tuple_cross_pkg`、`cross-zpkg/struct_nested_layout_cross_pkg`、
   `struct_array.z42` / `struct_heap_inline.z42` / `struct_jit.z42`（interp + jit 都要验）、
   `struct_static_field*.z42`。

## 不做

- 不引入 `StructLeafKind.Erased`（见上，格式 bump 代价）。
- 不改 D1-a（引用叶子进侧表，不进字节区）——`struct-value-semantics.md:325-335` 已否决 D1-b。
- 不给 struct 加 base/vtable（`:167-172`）。
- 不做「读出即复制 + 禁止写穿」那个替代方案：它**修不了 ①②**（源变量在存入之后被改，读时拷的是
  已经被改过的那块 blob），只能修 ③。2026-09-22 User 已裁决走存入复制。
