# fix-silent-gates（第 1 批 · scripts）——让不会红的门真的会红

> 类型：`fix`（最小化模式）。
> 起于 #496 的教训：一个门**成功时不打印任何东西**，于是「查了 0 个」和「查了 60 个」
> 看起来完全一样，4 类漏检潜伏数月。据此对全仓做了系统排查（四路并行：scripts / 文档 /
> Rust runtime / z42 测试），本文件是**第 1 批：scripts 侧的门**。

## 排查方法

判据只有一句：**它真的会红吗？** 四种失效形态——① 空集合上真空通过；② 验证路径吞异常；
③ 分支恒成功；④ 门存在但没人调用。

⚠️ 审计基准：主树当时落后 `origin/main` **139 个 commit**，全部排查在 `origin/main`
（`a13c2262`）的独立 worktree 上做（呼应 [[verify-audit-baseline-is-current]] 的教训——
上次在滞后基准上做审计，5 条高优先级结论里 4 条是已完成的工作）。

## 本批修的四条

### 1. `package runtime`（desktop）从不跑 source-identity 门 —— **是 #496 自己留的洞**

`_packageRuntime` 的 desktop 分支在 `_pkgFinish` **之前**就 `return` 了，而
`_buildRuntimePackage` 结尾是裸 `return 0`。`_pkgFinish` 全仓只有 3 个调用点，这条路一个都没沾。
偏偏这个包装的正是规则表**前两类**（`libs/` + `native/include`），而且 CI 发布它。

更糟的是调用方注释写着 "host runtime pack owns its dir + **finishes internally**" ——
读起来就是「它自己跑了门」，实际没有。**注释主动误导，比没注释更坏。**

修：`_buildRuntimePackage` 结尾改调 `_pkgFinish`；注释改正。

### 2. 脱钩告警只 warn 不红 —— 同样是 #496 的不彻底处

`_pkgSourceIdentityCheck` 里「规则路径存在、却 0 个文件可比对」原本只打 `⚠` 到 stderr、
**不计入 `Mismatch`**。也就是说：**本门要防的那个形状自己复发时，门是黄的、不是红的。**

修：`n == 0` 计入 `Mismatch` → 规则失联直接让 package 命令挂掉。

### 3. stdlib 孤儿源守卫**装错了路**

2026-09-06 加的 False-GREEN guard（为「z42.ir / z42c.core / z42c.syntax 静默数月」而加）
条件是 `nReq > 0 && totalFiles == 0`，**两条独立原因**让它在 GREEN gate 路径上永不触发：

- `nReq > 0` = 只在**显式点名 lib** 时查；gate 走的是不点名全扫 → `nReq == 0` →
  内层 `while (qi < nReq)` **空转**，`orphaned` 恒为空；
- `totalFiles` 是**全局**计数，全扫时必然 > 0，条件根本不成立。

修补存在，却没装在漏过去的那条路上——与 #496 完全同形的复发。

修：挪进 `_runLibKind`，改**逐 lib 判定**（该 lib 产出 0 单元、但 `<subdir>/` 下确有 `.z42`
源 → 红）。返回 `-1`，调用方既有的 `f < 0 → return 2` 直接接住（中止 stage）。

**⭐ 它第一次跑就抓到一条真的**：`z42.scripting/tests/` 有 **11 个 `<name>/driver.z42`，
零个 `[Test]`** —— `Main` 型 REPL 驱动，两种发现规则都不认领，**实际没有任何东西在跑**。
这笔债此前只存在于 memory 的一行笔记里（`fix-repl-eval-exception-program`「遗留 driver.z42
未接 gate」），现在门把它逼到了每轮输出上。

接通这 11 个 driver 属那条程序的范围（REPL e2e 需要真 tty），不在本 change 内。按本仓
`test lines` 的**棘轮**惯例处理：`_isKnownOrphanLib` 列已知欠债，**每轮响亮打印 `⚠`（不静默）
但不阻断；新增的孤儿源一律红**。

### 4. `test dist` 全 SKIP 也返回绿

三个 smoke 套件（launcher / desktop-publish / z42i）缺件时都是「静默 SKIP + 返回 `[0,0]`」，
而终判只看 `overallFail > 0` → **全部 SKIP 同样返回 0**。而 preflight 只断言了
`bin/z42c` / `bin/z42vm` / `libs`，**没有**断言 `z42`、`programs/launcher/launcher.zpkg`、
`bin/apphost` —— 恰恰是那三个 SKIP 判的东西。

SKIP 的条件字面上就是「被测对象从包里消失了」，而那正是这个 job 存在的理由。

修：把那三个路径提进 preflight，缺件直接红。

### 随手带的文档

`docs/book/src/dev/test-gate.md` 写「`lines` 守**文件 500 行硬上限**」——**不对**：500 只是
软限、只打 advisory、永不变红，硬限是 **886**（`_lineLimitHard()`；baseline 文件首行就写着
`hard limit 886`）。软/硬分档是 2026-09-05 在 code-organization.md 里有意做的，本页没跟上。

> 讽刺的是，这一页正是写着「没有测试盯着的约定迟早会烂」那句话的地方。

## 验证

### 正例

- `xtask test` **全绿 10/10 stage**，`⚠ z42.scripting` 棘轮行如期出现在输出里。
- `xtask package sdk --no-build` → `✓ 60 file(s) match repo source`。
- `xtask package runtime --no-build` → **同样 `✓ 60 file(s)`**。此前这条路径**一个字都不打**。

### 负例——每条新守卫都必须真的会红

| 守卫 | 负例 | 结果 |
|---|---|---|
| 孤儿源（全扫路径） | 给 `z42.build` 造 `tests/main_driver/driver.z42`（`Main` 型、无测试标注） | `❌ z42.build: tests/ 下有 .z42 源，却没产出任何 test 单元`，rc=2 |
| 棘轮基线 | 同上一次运行 | `⚠ z42.scripting`（已知欠债，不阻断）与 `❌ z42.build`（新增，红）**同时出现** |
| 规则脱钩 | 临时把 `stdlib libs` 规则的 `SrcAbs` 指向不存在目录 | `✗ stdlib libs: libs 存在但 0 个文件可比对 —— 规则已与拷贝点脱钩` + `✗ FAILED`，rc≠0 |

### ⚠️ 负例本身失败了两次，两次都说明「不验就不知道」

1. **第一版探针假设错了**：以为目录模式要求文件名是 `source.z42`，塞了个 `notmain.z42`
   想造孤儿 —— 它被正常发现并 PASS。真实规则是 `_dirHasTestMethods`：目录下**直接**有
   `.z42` 含 `[Test]` 即可，**文件名无关**。
2. **第二版被自己的注释破坏**：改成 `Main` 型后仍不触发 —— 因为我在注释里写了「无 `[Test]`」
   这几个字，而 `_dirHasTestMethods` 是**朴素全文子串匹配、连注释也算**。

> 如果只写守卫、看它在正常情况下不吵就收工，就会以为它工作正常 —— 而**前两版探针根本没能
> 触发它**。这两次失败也让守卫的报错文案被改准了（原文案把规则写成了「须叫 source.z42」，
> 是错的；**门的提示不准就是下一次误诊的种子**）。

### 一次假红，已定性

`--no-build` 跑 stdlib 时 `Z42IrMurmur3Tests` 4 条失败（`undefined function
Z42.Project.ZpkgBuilder.SourceHashHex`）。**与本变更无关**：overlay 种子来自落后 15 commit
的树，而 origin/main 的 #490 刚把 zpkg 内容标识换成 MurmurHash3 并新增了该函数；本 diff 只碰
xtask 五个文件，射程内不可能有 `ZpkgBuilder`。从当前源重建 stdlib 后 332 全过。

## 状态

🟢 完成（第 1 批）。后续批次见程序 memory：Rust runtime 4 条、z42 测试 4 条、文档收缩若干。
