# fix-package-identity-gate — 任务与验证

> 类型：`fix`（DRAFT 已过 User gate：论证接受、`hello_c` 走「从文档删」）。
> 设计与论证见 [proposal.md](proposal.md)。

## 任务

- [x] 1.1 `_pkgSha256Check` → `_pkgSourceIdentityCheck`，改表驱动
      （`_PkgIdentityRule` / `_PkgIdentityTally` / `_pkgIdentityRules` / `_pkgIdentityWalk` / `_pkgIdentityOne`）
- [x] 1.2 规则表补上漏检的 4 类：iOS Swift + dummy.c、iOS/android 的 `include/` 真头、
      Android Kotlin（**递归**子树）、Android JNI + CMakeLists、wasm js
- [x] 1.3 `_pkgFinish` 文案：`SHA-256 invariant check:` → `source-identity check (package copies vs repo source):`
- [x] 1.4 汇总行打印比对文件数 + 逐规则计数；**规则路径存在却 0 文件可比 → ⚠ 告警**
- [x] 2.1 `docs/workflow/packaging.md` §4 重写（规则表 + 拷贝点 + 「vs 源 ⟹ 跨包一致且更强」论证 + 递归陷阱）
- [x] 2.2 `scripts/README.md` 两处 `SHA-256 invariant` → `source-identity 门`
- [x] 2.3 `examples/hello_c/main.c` 从文档删（User 裁决）——它在仓库里存在、是给读者看的
      嵌入示例，没进包是 2026-05-13 后包结构重构（9/13 包 → `sdk`/`runtime`/`workload-desktop`
      三包制）的文档残留，不是功能退化

## 实施中发现的两个陷阱（都已落进代码注释与文档）

1. **ios `Sources/Z42VMC/` 与 android `…/cpp/` 不能整棵目录递归比**：它们的 `include/`
   装的是 `_copyAbiHeaders` 拷进去的**真 runtime 头**，而源树同名目录里是
   `#include "../../.."` 的**转发 stub**——整棵递归比会**假红**。故这两处用显式文件规则
   （`dummy.c` / `z42vm_jni.c` / `CMakeLists.txt`）+ 单独一条指向 `src/runtime/include` 的
   include 规则。
2. **Android Kotlin 在 `z42vm/src/main/java/`，不是文档写的 `kotlin/`**，且**有嵌套**
   （`io/z42/vm/*.kt`）→ 目录规则必须递归。

## 验证

### 正例

`xtask package sdk --no-build` → rc=0：

```
source-identity check (package copies vs repo source):
  ✓ 60 file(s) match repo source (stdlib libs 58, C ABI headers 2)
```

（改造前这 60 个文件也在查，但**一个字都不打**——现在数量可见，规则失联会被看见。）

### 负例：每一类新增规则都必须真的会红

**这是本次最关键的一步**：新增规则若不会红，等于又加了一堆死规则——正是本次要修的
「假保障」形状。`_pkgSetupDir` 每次重置包目录，「打包→篡改→重跑」行不通（重跑会重拷
覆盖篡改），故按本程序既有的**临时探针**配方（第 2 批 enumdump / 第 3 批 CLI help dump）
临时加一个 `xtask test pkg-identity <dir>`，对造出来的假 pkgDir 直接跑检查，**验完即删**
（harness 在 scratchpad `negcases.sh`，不进仓库）。

| # | 规则类 | 先摆成与源一致 | 篡改一字节后 |
|---|---|---|---|
| 1 | iOS Swift(7) + dummy.c + ios include(2) | ✓ 10 files, rc=0 | ✗ `Sources/Z42VM/Z42TestHost.swift differs`, rc=1 |
| 2 | iOS `Z42VMC/include` 真头 | ✓ 2 files, rc=0 | ✗ `…/z42_abi.h differs`, rc=1 |
| 3 | Android Kotlin **递归**(7) + JNI + CMake + include | ✓ 11 files, rc=0 | ✗ `z42vm/src/main/java/io/z42/vm/Z42VMEntry.kt differs`, rc=1 |
| 4 | Android JNI 单文件 | ✓ 2 files, rc=0 | ✗ `…/z42vm_jni.c differs`, rc=1 |
| 5 | wasm js(3) | ✓ 3 files, rc=0 | ✗ `js/index.js differs`, rc=1 |
| 6 | 死规则告警（`js/` 在、0 可比文件） | — | ⚠ `js 存在但 0 个文件可比对 —— 规则可能已与拷贝点脱钩` |

第 3 例同时证明了**递归子树**确实走到（篡改的是嵌套三层的 `io/z42/vm/*.kt`）。

### 回归

- `xtask test` 全绿 10/10 stage（不动点 3/3 gen1==gen2）
- `xtask test packages` 三层自检全 PASS（packaging 自检层，**不在** GREEN gate 内 → 手动补跑）
- 探针已从 `scripts/cli/xtask_cli_test.z42` 删净（`grep pkg-identity` 空；`git diff origin/main`
  对该文件为空 = 逐字节回到原样；`xtask test pkg-identity` 回落到 usage）

### ⚠️ 踩到一次假红：`package sdk` 跑在 `xtask test` **之前** → 不动点红

第一轮验证时顺序是 `package sdk --no-build` → `xtask test`，结果自举不动点 stage 报
3 个成员 gen1≠gen2（`z42c.semantics` 827144→823914B 等，差几 KB，不是 BLID 级抖动）。

**判定为与本变更无关**，依据三条：

1. 本次改动只碰 `xtask_package.z42` 的校验函数 + 文档；探针文件 `git diff origin/main` 为空
   —— **没有一行能影响 z42c 的编译输出**。
2. 单独重跑 `xtask test compiler` → **3/3 gen1==gen2 通过**。
3. 去掉前置 `package sdk`、重跑完整 `xtask test` → **全绿 10/10，不动点 3/3**。

**机制（一次观测 + 合理推断，非定论）**：`package sdk` 会经 `_z42cBuildToml` 往
`src/compiler/z42c.driver/publish/` 重建 z42c，把 in-tree 编译器产物搅成混合态，
使 GREEN 构建波里的 gen1 不自洽。

> **实践规则：验证顺序是「先 GREEN，后 package/build 冒烟」，别反过来。**
> 上一个 change（scripts-batch3-layering）恰好是这个顺序，所以没撞上。

## 状态

🟢 完成
