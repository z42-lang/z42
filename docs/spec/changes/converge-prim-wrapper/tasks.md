# tasks: converge-prim-wrapper

> 类型：**refactor**（收敛判据；实测零行为变化）｜ 创建：2026-09-27
> 出身：[结构审计 2026-09](../../../internals/src/compiler/architecture.md) 的 R2 第一条
> （「三份 `_primWrapper`，已实测不一致」）。

## Why

「prim 名 → 包装类短名」这一个判断有**三份并行实现**，而且**已经不一致**：

| 实现 | `object` | 非内建名的回退 |
|---|---|---|
| `PrimModel.Wrapper`（权威，头注自称「收敛历史七张并行映射」）| `Object` ✓ | 原样返回 |
| `TypeFactsTc._primWrapper` | 无分支，靠 `_capFirst("object") == "Object"` **恰好**答对 | **`_capFirst(n)`** ⇒ `point` 被猜成 `Point` |
| `EmitContext._primWrapper` | 无分支、无 `_capFirst` ⇒ 返回 `object`，**真的答错** | 原样返回 ✓ |

另外 `PrimModel.Wrapper` 多一层 `Canon`（剥 `?` 与 `Std.` 前缀）⇒ `int?` / `Std.Int32` 也归一到
`Int32`，而三份旧实现对这两种拼写各给不同答案。

两条附带事实：

- **`_capFirst` 不是归一化，是有损猜测**：类名 `point` 与 `Point` 是两个不同的类，猜中了就是
  一次**假命中**（`LocalClasses.ContainsKey` / `Symbols.HasClass` 都按这个名字查）。
- `EmitContext._primWrapper` 的注释写着「镜像 `TypeChecker._primWrapper`」，而**那个函数早已
  不存在**（`TypeChecker` 里只剩一句提到它的注释）。一份声称镜像某处的实现，其镜像对象是个鬼 ——
  「注释即第二份真相、且先于代码腐坏」的标准形态。
- 同一个 `EmitContext.z42` 里，**第 285 行早就在直接调 `PrimModel.Wrapper`** ——
  两种写法并存，说明委托是既有方向，不是我发明的。

## What Changes

后两份改为**转发** `PrimModel.Wrapper`；`_capFirst` 随之成为死码，删除。

## 🔴 定性：这**不是** bug 修复

实测下来三处分歧**都不是可观察的活 bug**，必须如实说，别在别处引用成 bug 修复：

- **自举不动点 3/3 逐字节一致** ⇒ 在编译器自身源码上零行为变化；
- 用 `class Point` + `class point` **并存**的可执行探针（正是 `_capFirst` 唯一能咬到的形态），
  在**新旧 driver 上输出相同**（`2/1`）⇒ 那条假命中没有走到能改变结果的地方。

所以本 change 的价值是：**三份真相收敛为一份**（R2 的主题）+ 删掉一份会被下一个人当归一化用的
有损猜测 + 改正一条指向已删除函数的注释。**latent hazard 的消除，不是 observed bug 的修复。**

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/TypeFactsTc.z42`（转发 + 删 `_capFirst`）
- `src/compiler/z42c.semantics/src/EmitContext.z42`（转发 + 改正注释）
- `src/compiler/z42c.pipeline/src/CacheStore.z42`（指纹 35 → 36）

## Tasks

- [x] `TypeFactsTc._primWrapper` / `EmitContext._primWrapper` 转发到 `PrimModel.Wrapper`
- [x] `_capFirst` 删除（全仓零引用；留着只会招人再拿它当归一化用）
- [x] 头注写清三份的差异表、`_capFirst` 为什么是猜测、以及那条指向鬼的镜像注释
- [x] `xtask test compiler` 全绿：单测全过 + **`✅ 自举不动点 3/3 gen1==gen2`（逐字节）**
- [x] 可执行探针新旧 driver 行为对比（`2/1` == `2/1`）
- [x] `CompilerFingerprint` 35 → 36
- [ ] GREEN：CI 全矩阵绿

## 为什么仍要 bump 指纹（尽管零观察变化）

映射答案对**三类输入**变了（非内建小写名 / `object` 在 EmitContext 侧 / `int?`·`Std.Int32`
经 `Canon` 归一）⇒ 同一份源码的编译结果**理论上**可与旧编译器不同 ⇒ 旧缓存条目有误命中风险。
判据取「同一份源码的编译结果（含诊断集）**是否可能**变」而非「我这次是否观察到变」——
后者依赖我的探针覆盖面，而覆盖面是最容易高估的东西。

⚠️ 这一档 **CI 的 fingerprint 守门必然是瞎的**（产物逐字节不变），按 version-bumping 规则表
第 1 行手动 bump。

## 不做（Out of Scope）

- **不动 `PrimModel` 的其余投影**（`Canon` / `SurfaceName` / `Code` / `IsScalarValue` 等）。
  审计点名的还有「六份基元名单」（编译器侧 + VM 侧各几份），那些跨语言、要先决定「判据下沉为
  数据位」的格式通道（审计裁决项 D-4），是另一条线。
- **不去查 `_capFirst` 假命中在别处有没有可达路径**。本 change 的正确性不依赖那个答案 ——
  转发之后那条路径根本不存在了。
