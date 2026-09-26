# tasks: fix-e2e-tmp-per-tree

> 类型：**test**（测试基建；不改被测行为）｜ 创建：2026-09-27
> 出身：做 `fix-flow-accessor-bypass` 时被这条误导了一轮。

## Why

`xtask_compiler_e2e_deploy.z42` 里 **8 个 fixture 根**都是 `/tmp` 下的**固定名字**
（`/tmp/z42c-e2e-deploy`、`-probing`、`-zref`、`-deployuse`、`-pparr`、`-z42bclos`、
`-deploydecl`、`-closure`），而**每个 fixture 开头都 `Directory.Delete(tmp, true)`**。

⇒ 同一台机器上并行跑两棵 worktree 的 `xtask test compiler`，**后进入某一格的那棵会把前一棵
正在用的中间产物整目录删掉**。

### 实测形态（2026-09-27）

`probing 数组` 那格红在 `pparr_b` 上：

```
✗ probing 数组: 预建 pparr_b 失败: cached: 0/1 files
z42c build: [dependencies] `z42.core` 未找到 z42.core.zpkg；已查找：/tmp/z42c-e2e-pparr/libs
```

而事后去看，那个 `libs/` 目录**根本不存在** —— `pparr_a` 已经建成（`sh_pparr_a` 在），
说明 `libs/` 当时是好的；它是在 `pparr_a` 与 `pparr_b` 之间被**另一棵树的同一格**删掉的。
当时 `wt-learn19` / `wt-tryget` 的 scratch 目录 mtime 都比本树更新，确有并发。

**取证**：同一份源码原样重跑 ⇒ `rc=0`、`✓ probing-paths 数组` 通过。

⚠️ **这类红最坏的地方是它看起来像被测对象的 bug**：报的是「依赖找不到」，指向清单 / 依赖解析，
而真正的原因在测试基建的共享状态。我因此先怀疑了一轮自己的改动。

## What Changes

新增 `_e2eTmpRoot(root, name)`：`/tmp/z42c-e2e-<树名>-<fixture 名>`，8 处全改调它。

- **为什么不搬进 `artifacts/.scratch/`**：这些 fixture **刻意住在仓库外** —— 它们要覆盖
  「repo 外的消费方」这条路（`_srcRoot` 上溯找不到 `libraries/+compiler/` ⇒ 返回 `""`，
  见 `_e2eDeployRoot` 原注释）。搬进仓库会把被测场景本身改掉。
- **树名取 `Path.GetFileName(_root())`**（`wt-cachekey` / `z42`）而不是哈希：这些路径会出现在
  诊断文本里，**可读比唯一性更有用**；同名不同父目录的两棵树仍会撞，但那比现在「所有树必撞」
  严格更好。空名（理论上的边界）回落 `z42`。

## Scope（允许改动的文件）

- `scripts/build/xtask_compiler_e2e_deploy.z42`（helper + 8 处）
- `scripts/build/xtask_compiler_e2e.z42`（漏掉的 4 处裸字面量）

## Tasks

- [x] `_e2eTmpRoot(root, name)` + **12 处**改调（`_e2eDeployRoot` 也跟着带上 `root` 形参）
- [x] 逐个确认调用点所在函数的作用域里都有 `root` 形参（都有）
- [x] 🔴 **第一轮漏了 4 处**：`xtask_compiler_e2e.z42` 里的 `buildhello` / `buildtext` /
      `buildclos` / `buildmulti` 写成**裸字符串字面量** `"/tmp/z42c-e2e-buildhello"`，
      而我第一轮 grep 用的是 `Path.Join("/tmp"` ⇒ 抓不到。
      **「认字符串不认语境」的批量改写盲区**（[[z42-batch-rewrite-context-blindness]] 那一族），
      我自己差点交一个只修一半的修复。是**跑完测试后列 `/tmp/z42c-e2e-*` 目录**暴露的：
      8 个带树名、4 个不带 ⇒ 判据是「产物清单」而不是 grep。
      收尾判据：全仓 `"/tmp/z42c` 与 `Path.Join("/tmp"` 只剩 helper 自己那一行。
- [x] 头注写清：为什么必须按树分、为什么不能搬进仓库、为什么用树名而不是哈希，以及那次实测形态
- [x] `xtask test compiler` 全绿（`rc=0` + `✅ 自举不动点 3/3 gen1==gen2`）；`/tmp` 下生成的
      **12 个根全部带树名**（`/tmp/z42c-e2e-wt-cachekey-*`）
- [ ] GREEN：CI 全矩阵绿

## 不做（Out of Scope）

- **不给其余 `/tmp` 用法做同样处理**。改完之后全仓只剩 helper 自己那一行提到 `/tmp`；别处的临时
  目录走的是 `artifacts/.scratch/`（仓库自己的约定）或 session scratchpad，本来就按树分开。
  📜 仓库此前已为**同一类问题**搬走过一处：`xtask_test_cross.z42:149` 的注释记着
  「Fixed scratch path (not a /tmp TempDir) …every run leaked a /tmp dir」⇒ 改用
  `artifacts/.scratch`。本 change 是那条迁移的收尾（这几处因为要「仓库外」而不能照搬那个做法）。
- **不加「fixture 根被外部删除」的自检**。那是给共享状态打补丁；根治是不共享。

## 并发的实时证据（2026-09-27 验证时顺手拿到）

跑全量验证**之前**刚 `rm -rf /tmp/z42c-e2e-*`；本轮只会创建带树名的路径。跑完去看 `/tmp`：

```
/tmp/z42c-e2e-wt-cachekey-{deploy,probing,pparr,zref,deployuse,z42bclos,deploydecl,closure,
                           buildhello,buildtext,buildclos,buildmulti}   ← 本树（12 个）
/tmp/z42c-e2e-{deploy,probing,pparr,zref,deployuse,z42bclos,deploydecl,closure}  ← 另一棵树（8 个）
```

那 8 个不带树名的根**只能来自另一棵树此刻在用旧代码跑同一套 e2e** —— 即修复前两轮必然共用
同一批目录、且各自都会 `Delete(tmp, true)`。这就是那次假红的现场。

## 验证

- 纯测试基建，不改任何产物字节、不改被测行为 ⇒ 无格式 bump、无指纹 bump。
- 判别力：修复前两棵树并发必然互相抹（已实测一次）；修复后路径互不相交。
  ⚠️ 这条没法写成一道自动门禁 —— 要复现得真的并发跑两棵树的全量 `test compiler`。
  取舍：路径隔离本身是**按构造**成立的（不同字符串），不像判据类改动需要阴性对照。
