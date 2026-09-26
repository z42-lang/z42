# tasks: fix-incr-generic-body-surface

> 类型：**fix**（最小化模式）｜ 创建：2026-09-26
> 出身：[结构审计 2026-09](../../../internals/src/compiler/generics.md) 的止血项 U-1。
> **这是本次审计里唯一一条实测坐实「静默产出错答案」的缺陷。**

## Why

`SurfaceHash`（文件级增量的失效闭包）把每个方法体折成 `{}`，于是「只改函数体 ⇒ 零传播」。
它的正确性论证里逐字写着：

> 泛型走运行期类型实参**不单态化进调用方**

这句话在 `complete-generic-instantiation` S1（#820 / #825）之后**不再成立**：
`IrGen._emitSpecializedFreeFn` / `IrGenTypeEmitter.EmitInstantiation` 把泛型体的特化副本发进
**实例化点所在 CU 的 sink**（即消费方的 IrModule），偏移按实例化布局烘焙。于是消费方的
`.zbc` 里烘焙着生产方泛型体的一份副本 —— 而「体」正是被折叠掉的那部分。

### 实测（修复前，release，两文件工程）

```
gen.z42:  public static long Second<T>(Loc<T, long> p) { return p.b; }
use.z42:  ... t.b = 7; return G.Second<P2>(t);      → 7
```

只把 `gen.z42` 的**体**改成 `return p.b + 100;`（签名一字不动）：

| 构建 | cached | 结果 |
|---|---|---|
| 增量 | `cached: 1/2 files` | **7**（错） |
| 同源全量 | — | **107**（对） |

消费方 `use.zbc` 里留着旧体的特化。**静默错答案，不是崩。**

## What Changes

`SurfaceHash` 对**泛型声明不折叠体**，判据集中在新增的 `_isGeneric(TypeParamList)`：

| 形态 | 谁会抄走这个体 |
|---|---|
| 声明自带型参（泛型自由函数 / 泛型方法）| `_emitSpecializedFreeFn` |
| 所属类型自带型参（泛型 class / struct 的成员；`impl ... for G<T>`）| `EmitInstantiation` |

体 token 留在该名字的指纹里 ⇒ 体一改指纹就变 ⇒ 提到它的文件照旧失效。
这正是本文件头注写明的**保守方向**（「漏记一种带体的声明形态 → 多编不错编」）。

## Scope（允许改动的文件）

- `src/compiler/z42c.driver/src/SurfaceHash.z42` —— 判据 + 修正已腐坏的正确性论证。
- `scripts/test/xtask_test_incremental.z42` —— 门 `_reconcileGenericBodyTouch` + 夹具 `_genBodyDir`。
- `docs/internals/src/compiler/generics.md` —— 记下混合模型与增量失效不变式。

## Tasks

- [x] `_isGeneric` 判据 + 三处不折叠（自由函数 / 类成员 / impl 成员；属性与索引器随 owner）
- [x] 修正 `SurfaceHash` 头注里那句已不成立的论证（它是「注释即第二份真相且已腐坏」的样本）
- [x] 门 `_reconcileGenericBodyTouch`：改泛型体后 **增量 dist == 全量 dist 逐文件字节**，
      外加一格**阳性对照**（改完必须与改前不同，否则门没有判别力）
- [x] 自带夹具 `_genBodyDir` —— 现有三份语料**没有**「跨文件泛型实例化 × blob struct 实参」
      这一格，而那是特化被触发的唯一条件
- [x] 实测阴性对照（种子旧 driver）：`cached: 1/2` + `use.zbc` 增量≠全量 ⇒ 门会红且点名消费方
- [x] 实测阳性（本修复）：`cached: 0/2` + 增量与全量逐文件相同
- [ ] GREEN：`xtask test incremental` 本地过；CI 全矩阵绿

## 代价与取舍

**保守方向的代价是过度失效**：改一个泛型类型的成员体（如 `List<T>.Add`）会失效**所有提到
`List` 的文件**。在编译期单调化下这是正确且不可避免的 —— 消费方确实可能烘焙了那份体。

精确做法（记 Deferred，不在本 change）：给每个 CU 的 `.meta` 记一条**反向边**「我特化了哪些
泛型体、它们来自哪个文件」，只在那些文件变化时失效。它精确但要新增 meta 字段与格式考量；
本 change 先取正确性，代价用 `test incremental` 的 D8 计时量出来记在 PR 里。

## 不做（Out of Scope）

- **不改特化副本的落点**（让它发在声明方而不是消费方）。那会动 IR 发射与跨 CU 装配，
  量级完全不同，且 S2 跨包投送的方向还没定（见审计裁决项 D-2）。
- **不动 `generics.md` 里「代码共享」那节的整体叙述**。该页仍按纯代码共享写，与 S1 之后的
  混合模型有更大范围的漂移；本 change 只在它后面补一节说清混合与失效不变式，不重写全页。

## 验证

- 无格式 bump、无指纹 bump：只改**哪些文件要重编**，不改任何产物字节；旧 `.meta` 里按老规则
  算的指纹与新规则算出的不等 ⇒ 当作「变了」⇒ 重编，方向安全、自愈。
- 自举字节不动点不受影响（surface hash 不参与发射）。
