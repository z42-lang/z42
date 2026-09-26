# Tasks: 接口成员的 `?` 标记跨包携带

> 状态：🟢 已完成 | 创建：2026-09-26 | 完成：2026-09-26
> 分支/worktree：`carry-iface-null-marks` @ `wt-ifacenull` | 基于：origin/main `27d5cdc44` (#862)
> 类型：`fix`（**compiler** —— 骑现有类级 attr-ref 通道，**无格式 bump**）
> 授权：口令「推进可空类型」（本线可选项 ③）

## 这是可空线最后一个「跨包漏报」的缺口

`define-null-check-marks` 的 `?` 标记，**方法**那半跨包已通（#791 形参 `$Nullable` /
返回值 `$RetNullable`，#806 extern 桩），**字段 / 属性**那半 #850 通了，只剩
**接口成员**：导入的接口方法一律 `RetIsNullable = false` / `IsNullable = false`
⇒ 两个消费端同时漏报 —— 调用点（`BoundCall.RetIsNullable`）与 D7 继承一致性
（`InheritanceResolver._checkOneIfaceMethod`）。

## ⭐⭐ 先纠正一条记错的代价评估（本线**第二次**同款）

旧记录说这条「接口走 **TYPE 方法块**、**没有 attr-ref 通道** ⇒ 要扩格式 + minor bump，
独立 change」，据此排成本线**最贵**的一条。**实测：不必扩格式。**

| 旧评估 | 实测 |
|---|---|
| 接口没有 attr-ref 通道 | 接口**方法块**确实没有，但**类级** attr 块是**无条件**写的（`ZbcWriter` 类型参数块之后紧跟 `WriteU16(cd.AttrCount)`），接口的 TYPE 记录早就带着它 —— 只是 `_interfaceDesc` 从不填（恒 0 条）|
| 要扩格式 + minor bump | **零格式 bump**。`$Caller:<kind>` 已有「哨兵带 payload」的先例；反射侧还有**通用 `$` 前缀过滤**（`corelib/reflection/attributes.rs`）⇒ 新哨兵自动不可见，不必逐个登记 |
| 做完 D7 的一致性检查才完整 | **消费侧本来就接通了**：接口方法走的是和类方法**同一个** `_fillParamMeta` 漏斗（`ImportedSymbolLoader`），DTO 上 `ExportedMethodZ.RetIsNullable` / `ExportedParamZ.IsNullable` 两位现成；D7 的 `_checkNullableDirection` 也没有任何「导入接口跳过」的守卫 |

⇒ 真实工作量 = **一条已存在通道的两个端点**（发哨兵 + 解码）。

⭐ **教训（与 #850 一字不差，所以它是规律不是巧合）**：**复查一条「贵」的评估，先看它给的
是实现描述还是原则**。两条都是实现描述，而且都是在「旁路通道还不存在」的年代写下的
（`$Nullable` 通道 #791 才建、`$IfaceNull` 要用的类级块当时也没人往接口上填过）。
⚠️ 本线剩下的 ④（TOML API 形状）是**原则**类判断，不适用这条。

## 落地形态（四处，全是现成的）

| 环节 | 落点 | 现成先例 |
|---|---|---|
| 哨兵常量 | `z42.ir/IrModule.z42` 的 `IrIfaceNullable`（`$IfaceNull`，payload `"<mi>:r"` / `"<mi>:<pi>"`）| `IrDeprecation` / `IrNullableRet` |
| 生产侧发哨兵 | `ClassDescBuilder._interfaceDesc` → `cd.Attrs`（方法 / 属性 get·set / 索引器 get·set 五条路）| `$Deprecated` 挂类级 |
| 解码 | `TsigReconcile._rebuildInterface` | 同函数里 `AssocTypeNames` / `BaseNames` 的搬运 |
| 导入侧回填 | **无需改动** —— `ImportedSymbolLoader` 已调 `_fillParamMeta(msig, mz.Params, …, mz.RetIsNullable)` | 同类方法那条 |

⭐ **为什么不能走类型拼写**：`ClassDescBuilder._typeFieldName` 对 `NullableType` **显式取 `Inner`**
（与 SIGS 的 `_sigTypeName` 同理）——那串拼写同时是**查找键**（导入侧 `_resolve` 按它找类型），
塞 `?` 进去就改了键。这与 #791 在 SIGS 上踩的是同一个坑，两处独立地得出同一个结论。

## 顺带补齐：本包那半（接口属性 / 索引器）

`MemberCollector._fillInterface` 里，接口**方法**走 `_sc._methodSymbol`（统一填标记位），
而**属性 / 索引器**的访问器是**手搓 `Z42FuncType`**、四处一直没填 ⇒ 接口属性标了 `?`
在**本包**也不受检。

⚠️ **这不是夹带**：跨包那半一旦接通，导入侧会从 `$IfaceNull` 读出标记并强制检查，而声明包
自己不检查 —— **导入方比声明方还严**。两边必须同时有。

## 进度概览

- [x] 1 携带机制（哨兵常量 + 发射 + 解码）
- [x] 2 本包那半：`_fillInterface` 的属性 / 索引器四处补填标记位
- [x] 3 跨包 fixture：正例 + **两条负例**（两个消费端各一条），并做**阴性对照**
- [x] 4 全量 GREEN + 指纹 + 文档 + PR

## 3 三条 fixture，两条负例各压一个消费端

- **正例** `iface_null_mark_crosspkg`：方法返回位 / 接口**属性** getter / 🔒 未标对照各一，
  消费方先存局部再查 ⇒ 编过并跑出预期输出。
- **负例①** `iface_null_mark_crosspkg_unchecked`：经接口收者拿 `?` 返回值**直接解引用**
  ⇒ 期望 **E0478**。压的是**调用点**（`BoundCall.RetIsNullable`）。
- **负例②** `iface_null_mark_crosspkg_impl`：实现**跨包**接口时把形参的 `?` 去掉
  ⇒ 期望 **E0489**。压的是 **D7**（`_checkOneIfaceMethod` → `_checkNullableDirection`）。

🔴 **为什么负例不可省**：全仓标了 `?` 的接口成员原本是 **0 处**（实测：100 个接口声明 /
121 个成员行 / `?` 零处）。只留正例的话，机制**完全没接通**时它照样全绿。

## ⭐⭐ 阴性对照抓到：负例②初稿在为**错误的理由**通过

按本线铁律，阴性对照取**撤回修复本身**（不是改期望值）：把 `_interfaceDesc` 里
`cd.Attrs = nullRefs;` 那一行注释掉重建全量。

| 轮次 | 正例 | 负例① unchecked | 负例② impl |
|---|---|---|---|
| 撤回发射端（初稿 fixture）| PASS | **FAIL** ✅ | **PASS** ❌ |
| 撤回发射端（修正 fixture）| PASS | **FAIL** ✅ | **FAIL** ✅ |
| 机制在 | PASS | PASS | PASS |

初稿里 `MyFinder` 照抄契约把 `Find` 写成 `string? Find(...)`、`Current` 写成 `string?`。
机制没接通时导入契约的标记位全是 false ⇒ 实现**加** `?` 撞上 D7 的**另一条**规则
（「实现方加 `?` 禁止」）⇒ **照样报 E0489、照样 PASS**。

⇒ 这正是记忆里 `nullable_marks_cross_pkg_override` 警告过的那条「反方向修前就能报」，
**换了个载体（接口而非 override）又重现了一次**。修法：实现方**一处都不加 `?`**，
只留 `Take` 形参去 `?` 这唯一一处不符；返回位反而**去掉** `?`（承诺更强，D7 允许）
⇒ 顺带成了两格 🔒 对照。

⭐ 教训：**「负例变红」只证明它会红，不证明它为你想要的理由红**。
判别力要靠阴性对照逐条确认，而阴性对照必须**只变一个变量**（这里就是发射端那一行）。

## 4 收尾
- [x] 4.1 `xtask test` 全量 GREEN（**9m28s / 15 stage 全过**）
- [x] 4.2 指纹 32 → **33**（理由写在 `CacheStore.z42` 常量注释里；合并前按当时的 main 现查复核）
- [x] 4.3 文档：`reference/language/types.md` 跨包那段（「只有接口成员不跟」已不成立）
      + `internals/compiler/attribute-pipeline.md` 补**内部哨兵一览**与「为什么挂类级」
- [x] 4.4 归档（阶段 9，在本 PR 内）→ `archive/2026-09-26-carry-iface-null-marks/`
