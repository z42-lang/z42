# Tasks: blob struct 返回位的 ABI 统一（PR 1/2 —— 接口/型参边界）

> D1 已定（A′⁺，触发面收窄见 design D1′）/ D2 已定（分两刀）/ **D3 仍待裁决**（⑤ 的路线）。

## 0. 取证（DRAFT 阶段，全部实跑）

- [x] ④a 在 main 上复现：双字段 `Vec2` + `where T : INumber` → `takes 3 physical, passes 2`
- [x] **调查中新发现第三副面孔**：`interface IClone { Self Copy(); }` 被双字段 struct 实现，
      经接口收者调用 → `takes 2 physical, passes 1`（与 ④a 同根，判据同一条）
- [x] ⑤ 爆炸半径实测：`IsBlobStruct` 翻 `< 1` ⇒ 编译器自举**构建成功**、e2e **355/2**
      ⇒ 🔴 推翻旧记录「翻 `>=1` 会 AllocStrong arity 崩」
- [x] ⑤-b 性质：真用例 `gc_handle.z42` 的调用点 IR 是 `vcall %27.AllocStrong(%26)`（按名字派发）
- [x] 单字段 struct 普查 `{0:36, 1:10, 2:33, 3:4, 4:2}`；发布库里只有 `GCHandle` / `Guid`
- [x] `INumber` 的 stdlib 实现者**只有基元 wrapper**（scalar ⇒ `CanBridge` 判假）⇒ stdlib 产物不变

## 1. 实施

- [x] `InheritanceResolver._checkOneIfaceMethod`：打标条件加 `declRetTypeParam`
      （**看未代换的 `ims.Signature.Ret`**）；型参位的桥接返回类型取 `object`
- [x] `IfaceBridgeSynth.EmitBridge`：泛化到 **N 形参 + 可静态**（原版硬编码「1 个 `this` + sret」，
      只够 `T GetEnumerator()`）；形参寄存器按真实类型登记 REGT（一律 Ref 会让 REGT 与实参形态不符）
- [x] `_initCtx` 拆两种形态（`CanBridge` 借的临时 ctx 无形参 / `EmitBridge` 要登记形参）
- [x] `IrGenMemberEmitter`：把 `bms` 与 `isInstance` 传进 `EmitBridge`
- [x] **四处静态调用点 `RegKey` → `CallKey()`**：`ExprTyper` 具体类运算符 + `MemberResolver` 的
      `Class.m` / prim wrapper / 限定名 + `MemberResolver.Bare` 的同类静态调用
      （#814 只改了实例路 —— 它的触发面只产实例方法；`CallKey()` 注释把「凡以 RegKey 作调用目标名
      的站点都该改用它」定为纪律）
- [x] `CompilerFingerprint` 22 → **25**（理由与让号实录写在注释里；#844/#845 双双取 23）

## 2. 验证

- [x] ④a 正面：`Add(p,q).X/.Y` = 11/22；`Sub`/`Mul`/`Chain3` 链式；赋给具体局部再读
- [x] 反向守卫：具体类型收者 `(p+q).X`、`Vec2 s = p+q`、**显式静态** `Vec2.op_Add(p,q).X`
      —— 它们必须仍走带 sret 的 `$struct`（打在无 sret 的桥接上会报 `takes 2 …, passes 3`；
      第三条正是这么暴露出「还有一处 RegKey 没改」的）
- [x] 不回归：`Money`（单字段、非 blob）、基元 `Add(20,22)` / `Add(1.5,0.5)`、class 实现、
      `IExact`（声明就写具体 struct ⇒ **不该**桥接）
- [x] **阴性对照（撤机制本身）**：把 `declRetTypeParam` 改成 `false` 重建 ⇒
      两条 golden 各回到 `Vec2.op_Add ... takes 3 …, passes 2` 与 `Vec2.Copy ... takes 2 …, passes 1`，
      **0 passed / 2 failed**（不是改期望值）
- [x] golden：`operators/static_abstract_operator.z42` 扩 Vec2 组（并订正其抬头 ——
      原文把 Money 当成「真正会炸的形态」，实测它是单字段、2 对 2 巧合通过）
      + 新增 `interfaces/self_return_blob_struct.z42`（含 `IExact` / `One` / `Node` 三组阴性面）
- [x] 受影响分类全绿：operators 40 / interfaces 18 / structs 4 / generics 60 / types 142（interp+jit）
- [ ] `xtask test` 全仓（bump 后重跑）
- [ ] `cargo test --lib`（debug、不带名字过滤）

## 3. 文档

- [x] `docs/internals/src/runtime/missing-symbol.md`（sret 的 SoT 页）：新增「谁负责传 sret」一节 ——
      一个根因三副面孔的表 + 收口不变式 + 为什么判据必须看未代换的声明 + 为什么不让 VM 自适应
- [x] `docs/reference/src/language/generic-constraints.md`：`where T : INumber` 对多字段 struct 可用
      + 新增「blob struct 的返回位」节（含「Money 是单字段 ⇒ 既有用例抓不到」的历史订正）
- [ ] `docs/roadmap.md`：本条落地 + ⑤/⑤-b 的依赖关系

## 4. 留给下一刀

- **⑤**（D3 待裁决）：取 ⑤-blob 则必须自带 **⑤-b 的判据**（「导出的、返回 blob struct 的方法」——
      `GCHandle.AllocStrong` 不实现接口，本刀的接口满足性打标够不到它）。
- **`substitute-generic-call-return-type`**（D2 已定分两刀）：本刀之后它才安全。
