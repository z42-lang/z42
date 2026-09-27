# tasks: fingerprint-content-derived

> 类型：**refactor**（编译器语义指纹改内容派生）｜ 创建：2026-09-27
> 出身：结构审计 2026-09 裁决项 **D-3**，User 2026-09-27 批准。

## Why

`CompilerFingerprint` 是一个手工维护的整数（换方案时到 41）。它有两条**实测损害，都在
2026-09-27 当天发生**：

| 损害 | 实录 |
|---|---|
| **撞号 / 让号** | 同一天两次：#883 原取 36 被 #885 抢走让到 37；#892 原取 38 被 #891 抢走让到 39。号是「先合先得」，后到的必须回头改自己的 PR |
| 🔴 **正文被整行覆盖而丢失** | **#897（指纹 40，E0403）的整条理由被 #898 的合并吃掉**。两个 PR 都基于 39、都改**同一行**，后合的整行胜出，**git 没报冲突**。main 上一度是 41 → 39，40 那一档凭空消失（`NeverCompletes` 在链里零出现）|

第二条不是假设的风险，是**已经发生的数据丢失**，本 change 把那条正文补回文档。

## What Changes

**指纹 = `CompilerFingerprint.Entries` 这张列表的内容哈希**（`ZpkgBuilder.SourceHashHex`）。
每条语义变更**追加一行自己的 slug**，不再手工 +1。

```z42
public static string[] Entries = new string[] {
    "baseline-41",                  // 换方案前计数器到 41 时的编译器语义
    "fingerprint-content-derived"
};
```

两条损害被**结构性**消掉：**没有号可抢**；两个 PR 各追加一行，合并只会**两行都留下**
（能自动合并则自动，冲突时正解是「都留」而不是「谁让号」）。

## 🔴 为什么不去哈希编译器全部源码（审计原本的提法）

**实测那会让每次注释编辑都全量失效**：

| 场景 | 墙钟 | 缓存 |
|---|---|---|
| 空跑（全命中） | 12.0s | 123/123 |
| 改一行注释 | 18.0s | 122/123 |
| **指纹变一次** | **26.8s** | **0/123 + 0/16 + 0/12** |

⇒ 内循环 **+50%**，还连带 stdlib 全量（`12.4s → 0.6s` 那条优化会被抵消）。
列表方案只在**人记录了一条语义变更**时才变，**零额外失效**。
代价：仍需人判断「这次要不要记一条」——与计数器时代相同，由 CI 守门兜底。

## 顺带改正一条我误报的「规范冲突」

我此前向 User 报过 D-1「`version-bumping.md` 第 1 行与第 3 行矛盾」，**报错了**：
第 3 行的主语是 **reader/writer**，不是「字节没变」；两行作用域不相交。
本 change 在规则表里写明了这一点，并把第 1 行显式补上「**发出的诊断**」——
此前只写「typecheck」，读的人容易把「发码没变」当成不必追加的理由，而那恰恰是最需要失效的一档。

同时改正 CI 守门表里「不变 ⇒ ✅（**纯重构、改注释**）」的括注：**只改诊断也落在这一格，
却不是纯重构**；那道门比的是产物字节，对「诊断变、发码不变」是**结构性地瞎的**。

## Scope

- `src/compiler/z42c.pipeline/src/CompilerFingerprint.z42`（新）
- `src/compiler/z42c.pipeline/src/CacheStore.z42`（删掉那条 16KB 单行常量；改为 `Fingerprint()`）
- `src/compiler/z42c.pipeline/tests/incremental/incremental_tests.z42`
- `scripts/test/xtask_test_fingerprint.z42`、`scripts/cli/xtask_cli_test.z42`（守门改读列表）
- `docs/internals/src/compiler/compiler-fingerprint.md`（新：机制 + 1–41 全部历史，含补回的 40）
- `docs/agent/rules/version-bumping.md`

## Tasks

- [x] `CompilerFingerprint.Entries` + `Value()`（哈希缓存一次）
- [x] `CacheStore` 5 处使用点改走 `Fingerprint()`；那条 16KB 单行注释移入文档
- [x] 补回 #897 丢失的那条正文（文档里显式标注它是怎么丢的）
- [x] CI 守门改读列表：条目数用于**打印**，指纹源文件**全文比较**用于**决策**
      （改了既有条目正文也看得见）
- [x] 实测三组：新指纹形如 `mmh3:49054591…`；**改注释指纹不变**（122/123）；
      **追加一条 slug 指纹就变**（0/123）
- [x] 换方案本身触发**一次性**全失效（`0/123` → 恢复 `123/123`），符合预期
- [x] `xtask test compiler`：24 unit 全过 + 自举不动点 3/3 gen1==gen2
- [x] `xtask test incremental` 全绿（xtask 80/80 + stdlib 25/25 逐字节）
- [ ] GREEN：CI 全矩阵绿

## 我栽的一次

守门里写了 `t.IndexOf("};", k)` —— **z42 的 `String.IndexOf` 没有 (needle, startIndex) 重载**，
`xtask test incremental` 的 xtask 那轮直接 `initial build failed`（它编的正是 xtask 源码树）。
改成先 `Substring` 再找。⚠️ 教训：**改了 `scripts/` 就必须先单独编一遍 xtask 源码**，
否则错误会以「某个 fixture 构建失败」的面目出现在离现场很远的地方。

## 不做（Out of Scope）

- **不改判据本身**（什么时候该失效）。判据仍是「同一份源码的编译结果（含诊断集）是否可能变」，
  由人判断 + CI 守门兜底。本 change 只换**表达方式**。
- **不做 token 流哈希**（自动判断「这次改动是否影响语义」）。那能连「忘记记一条」一起消掉，
  但要给全部编译器源码做词法扫描，且仍会对「重命名局部变量」这类无害改动过度失效。
  留作后续可选项。
- **不动 zbc/zpkg 格式**：指纹是纯 z42c 内部标识，不涉 wire。
