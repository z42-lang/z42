# Proposal: blob struct 返回位的 ABI 统一 —— 让「按名字派发」的边界不再要求调用点知道 sret

> 状态：✅ **PR 1/2 已实施（2026-09-26）—— 接口/型参边界那半**；⑤ 与 ⑤-b 仍待 D3 裁决。
> 类型：**lang + ir 约定**（不新增指令、不动 wire；改的是「谁负责 sret」这条编译期约定）。
>
> **本 PR 交付**：坑点 **④a**（泛型约束运算符派发）+ 调查中**新发现的第三副面孔**
> （实例 `Self Copy()` 经接口收者）。
> **不在本 PR**：**⑤**（单字段 struct 值语义）及其第二个阻塞项 **⑤-b**
> （跨包 loose VCall 的 `GCHandle.AllocStrong`）—— 后者不是接口成员，本 PR 的触发面（接口满足性
> 打标）够不到它，需要独立判据；⑤ 的路线（blob 化 vs 标量塌缩）仍是 **D3**，待裁决。

## Why

### 一个根因，三副面孔

sret（返回 blob struct 时由调用方传入的隐藏返回槽）今天**由调用点的静态返回类型决定**
（`CallEmitter.z42:43,190` 的 `_isBlobStruct(c.Type())`），而 callee 侧是**每方法固定的
`method_flags bit3`**，VM 按 `param_count + sret` 严格校验、失配即抛（`symres.rs:195`，
且 `symres_tests.rs:65` 专门断言**不许按 arity 自适应** —— `fix-call-arity-skew` 的门）。

⇒ **凡是「调用点看不见具体返回类型」的派发边界，这条约定就自相矛盾**。今天有三处实例：

| # | 形态 | 实测措辞 | 状态 |
|---|---|---|---|
| ④a | 泛型体内 `a + b`（`where T : INumber`），T 擦除 | `Vec2.op_Add ... takes 3 physical argument(s), the call passes 2` | ✅ **本 PR 修** |
| ④a′ | 实例 `Self Copy()` 经**接口收者**（调查中新发现） | `Vec2.Copy ... takes 2 physical argument(s), the call passes 1` | ✅ **本 PR 修** |
| ⑤-a | 单字段 struct 若按 blob 处理 ⇒ `Money` 变 blob ⇒ 同 ④a | 同上 | ✅ 本 PR 的桥接已覆盖（⑤ 一开就生效）|
| ⑤-b | 跨包 `GCHandle.AllocStrong(obj)`：调用点发 **loose VCall** 按名字派发 | `Std.GCHandle.AllocStrong$1$object ... takes 2 physical argument(s), the call passes 1` | 🔴 **仍未修**（非接口成员，触发面够不到）|

**实测取证（2026-09-26，把 `IsBlobStruct` 的 `FieldCount < 2` 翻成 `< 1`，即让单字段 struct 走 blob）**：

- 编译器**自举构建成功**（`BUILD_EXIT=0`）
  —— 🔴 **推翻旧记录**「翻 `>=1` 会 `AllocStrong` arity 崩」：那不是构建失败，是运行期 sret 失配。
- e2e goldens：**355 passed / 2 failed**，两条**全部**是上表 ⑤-a 与 ⑤-b，**没有第三种形态**。
- 调用点 IR 实证（用实验编译器 `--dump-ir`）：`%4 = vcall %2.AllocStrong(%1)` ——
  跨包返回 blob 的静态方法在调用点是**按名字的 VCall、无 sret 槽**。

⇒ 爆炸半径小且集中，但它**不是「补两个洞」，是一条约定需要收口**。

### 为什么现在做

- ④b 已合（#838）：`field_get` 现在接受装箱 struct ⇒ **桥接产出 `BoxedStruct` 这条路已经通了**
  （这正是旧记录里「做 ④a 之前要先确认 `id(v).X` 状态」的那个前置，已解除）。
- #814（`add-iface-return-bridge`）已在 main 把 **A′ 桥接范式**建好：`IfaceBridgeSynth.z42` +
  `IrGenMemberEmitter` 的 `emitKey = methKey + "$struct"` + `EmitContext` 的剥名规则。
  ④a 与 ⑤-b 是**同一个范式的另外两个触发面**，不是新机制。

## What Changes（本 PR 实际落地）

**把 #814 的 A′ 从「接口声明返回引用型」扩到「接口声明的返回位是型参（含 `Self`）」**：

> 返回 blob struct 的实现：具体实现挪到 `<m>$struct`（带 sret），裸名槽放合成桥接
> （`struct_alloc` + `call <m>$struct` + `__box_struct` + `ret`，返回引用）。
> 静态可解析的直接调用点经 `MethodSymbol.CallKey()` 绑 `$struct` ⇒ 无装箱快路径零开销、字节不变。

四处改动：

1. `InheritanceResolver._checkOneIfaceMethod`：打标条件加 `declRetTypeParam`
   （**看未代换的 `ims.Signature.Ret`**，见 design D1′）；桥接声明返回类型取 `object`。
2. `IfaceBridgeSynth.EmitBridge`：从硬编码「1 个 `this` + sret」**泛化到 N 形参 + 可静态**
   （`static abstract Self op_Add(Self,Self)` 是 2 声明形参 + 静态；形参寄存器按真实类型登记 REGT）。
3. `IrGenMemberEmitter`：把符号与 `isInstance` 传给 `EmitBridge`。
4. **四处仍以裸 `RegKey` 作调用目标名的静态调用点改走 `CallKey()`**
   （`ExprTyper` 的具体类运算符路径 + `MemberResolver` 的 `Class.m` / prim wrapper / 限定名三处，
   外加 `MemberResolver.Bare` 的同类静态调用）。#814 只改了实例路 —— 它的触发面只产实例方法。
   这正是 `CallKey()` 注释里那条纪律「凡以 RegKey 作调用目标名的站点都该改用它」。

**触发面刻意收窄**（不取 DRAFT 里建议的 T1「一律给所有 blob 返回方法建桥」）：判据挂在**接口满足性**
上（符号层打标），因此**够不到 ⑤-b**（`GCHandle.AllocStrong` 不实现任何接口）。
T1 的代价与收益在 ⑤ 的路线定下来之前评不准 —— 若 D3 取 ⑤-blob，⑤-b 需要一个独立判据
（「导出的、返回 blob struct 的方法」），届时再评是否升级成 T1。

## 与「返回位代换」的关系（D2 待裁决）

另有一条**已登记未做**的缺口 `substitute-generic-call-return-type`：
`MethodTypeArgSubst.ForExplicitTypeArgs` 拿 `TypeParamNames.**Length**`（= 解析器
`new string[4]` 的**容量**、`Count` 另记）当型参个数 ⇒ **本地声明**的泛型方法/自由函数上
显式 `<T>` 的签名代换**从未生效**（跨包导入的因元数据数组是精确长度反而生效）。

- 判别性探针：声明**恰好 4 个**型参时代换立刻生效（`id4<Vec2,int,int,int>(v)` 的 bound 由 `:T` 变 `:Vec2`）。
- 它**不能先修**：代换一生效，调用点按具体返回类型加 sret，而擦除的 callee 没有 ⇒ 立刻撞 ④a。
- 但本刀做完后它就**安全了**（桥接让裸名入口无 sret），且它能把 `id(v).X` 这类写法从
  「运行期靠装箱 + 反射式按名读字段」提升为「编译期就知道是 `Vec2`、发 `struct_fget_prim` 快路」。

⇒ **D2：本刀是否同刀放开返回位代换？** 我的建议是**分两刀**（先约定、后类型），理由见 design。

## 不做什么

- **不动 VM、不动 wire 格式、不新增指令**（#814 D1 已裁决过同一个问题：形状 B「VM 按目标 flags
  自适应」被否 —— 它把每方法固定的 ABI 变成派发时协商，且会让 JIT 对接口调用整体降级）。
- **不碰泛型特化线**（`wt-geninst` 有别的会话在做）。本刀只改「谁传 sret」，不改布局/特化。
- **不改 `symres.rs` 那道 arity 门**（`fix-call-arity-skew` 的判别力要保住）。
