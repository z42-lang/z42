# tasks: fix-devirt-bare-name-alias

> 类型：**fix**（去虚化发出指向不存在函数名的直接 Call）｜ 创建：2026-09-27
> 出身：`fix-inline-breaks-ref-params`（#891）铺开 `opt_all` 时炸出的第二簇，该 PR 明确留给本刀。

## Why

4 行可复现，`--opt-all` / `--release` 下必崩，默认优化集通过：

```z42
namespace Test;
using Std;
void Main() { Assert.Equal("Std.Type", typeof(int).GetType().FullName); }
```

```console
$ z42c --emit-zbc p.z42 p.zbc --opt-all && z42vm p.zbc Test.Main --mode interp
Error: uncaught exception: Std.MissingSymbolException: undefined function `Std.Type.GetType`
```

`--release --no-opt devirt` 通过 ⇒ **单变量锁定 Devirt**。interp 与 jit 都崩。

### 根因：谓词回答的是另一个问题

`DependencyIndex.AddModule` 给 mangle 名（`Name$N$T`）**额外注册裸名别名**
（`Cls.Name` 与 `ns.Cls.Name`，见 `DependencyIndex.z42` 那两处 `TryAdd`），供调用点按裸名消歧。

而 `EmitContext._depHasFunction(fq)` 只查 **`Statics.ContainsKey(fq)`**，它的两个调用方
—— `ResolveSealedTarget`（去虚化）与 `NarrowPrimTarget`（窄基元派发）—— **都把 `fq` 本身当直呼
目标名**。于是命中一个别名就会发出一条指向**不存在的函数名**的直接 Call。

本例的撞车形态很典型：

| 来源 | 声明 | 发射名 |
|---|---|---|
| `Std.Type` | `public static extern Type GetType(string fqn)` | `Std.Type.GetType$1$string` |
| `Std.Object` | `public extern Type GetType()` | `Std.Object.GetType` |

TSIG 把 `Object` 的**实例** `GetType` 展平进 `Type.Methods` 的**裸名**键，而 `AddModule` 又给
那个**静态**的 mangle 名注册了 `Std.Type.GetType` 别名 ⇒ 两者在裸名上撞车，校验被别名骗过。

### 🔴 顺带改正两处**假声明**

`optimization-pipeline.md:82` 与 `sealed.md:108` 都写着去虚化「**永不 miscall**」。
**那句话是假的** —— 本 bug 就是一格 miscall（而且是崩，不是错值）。两处已改正并写明教训：

> **「校验过了」要问清那个谓词回答的是哪个问题** —— 「这个键在表里吗」与
> 「有一个真叫这个名字的函数吗」，在**有别名的表**上不是同一件事。

## What Changes

`EmitContext._depHasFunction`：从「键存在」收紧为「**entry 的真名与 fq 逐字相同**」。

```z42
int i = this.Deps.Statics.Find(fq);
if (i < 0) { return false; }
DepCallEntry e = this.Deps.Statics.ValAt(i) as DepCallEntry;
return e != null && e.QualifiedName == fq;
```

非重载方法 `fq == QualifiedName` 照旧命中（**零回归**），只有「别名命中而真名不同」这一格被拒
—— 那一格本来就必错。一处修改护住**两个**调用方。

## 🔴 我第一次找错了根因，已撤回

第一版我猜是 **`MethodSymbol.IsStatic`**（「实例调用点不能去虚化到静态方法」），写了守卫、
建了编译器、跑了探针 —— **照旧崩**。实际 `Type.Methods["GetType"]` 里坐着的是从 `Object`
展平来的**实例**方法（`IsStatic == false`），守卫压根不触发。

那条守卫已**整条撤回**，只留证实过的修法 —— 不留一条自己没验证过它在挡什么的代码。
教训与本仓既有的「移除闸门/放宽判据前先确认它在挡什么」是同一条的反面：
**加闸门也要先证明它真的挡到了目标**。

## 指纹：37 → 39

**bump 的理由是发码变**：受影响的调用点从「直接 Call 到一个不存在的名字」改为回落 VCall
⇒ 同一份源码哈希不变而发码变，不 bump 就会命中旧条目、修复永不生效。

⚠️ **让号实录**：main 上是 37，在飞的 #891 取 38 ⇒ 本刀让到 **39**
（`parallel-development.md` §4.1「按合并顺序让号」）。合并前按当时的 main 现查复核。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/EmitContext.z42`
- `src/compiler/z42c.pipeline/src/CacheStore.z42`（指纹 37 → 39）
- `src/tests/types/value_type_object_methods.opt_all`（新，#891 刻意留下的那一个）
- `docs/internals/src/runtime/optimization-pipeline.md`、`docs/reference/src/language/sealed.md`

## Tasks

- [x] `_depHasFunction` 收紧为真名逐字相同
- [x] 4 行最小复现从崩溃变通过（`--opt-all` 与默认两档都对）
- [x] 原 fixture `value_type_object_methods` 在 `--opt-all` 下 interp + jit 全绿
- [x] 给它挂上 `opt_all`（#891 写明「修好 Devirt 时连它的 sidecar 一起加上」）
- [x] **自举不动点 3/3 gen1==gen2** + `test compiler` 零失败
- [x] `xtask test e2e` 740 passed, 0 failed；入库字节基线零漂移
- [x] 撤回第一版的 `IsStatic` 误判守卫
- [x] 改正两处「永不 miscall」假声明
- [ ] GREEN：CI 全矩阵绿

## 不做（Out of Scope）

- **不动 `DependencyIndex` 的别名注册**。别名本身有正当用途（调用点按裸名消歧），错的是
  消费侧把键当名字用。改注册会牵动所有按裸名解析的路径，风险远大于收紧一个谓词。
- **不加 arity 判据**。`ResolveSealedTarget` 收了 `argc` 却从未使用（全函数体零引用），看着像
  个缺口；但 `ParamCount` 与默认值 / 变长形参（`ParamsFrom` / `ParamDefaults`）交织，等值比较
  会把合法的带默认值方法也拒掉（静默丢优化）。**真名逐字相同**这一条已经覆盖了本 bug，
  且判据无歧义。`argc` 未使用一事记在这里，谁要动再评估。
