# 编译器语义指纹（`CompilerFingerprint`）

> 页型：机制页 ｜ 代码：`src/compiler/z42c.pipeline/src/CompilerFingerprint.z42`
> 追加纪律见本页 [什么时候要追加一条](#什么时候要追加一条)；格式 bump 清单见
> [version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md)。

## 它是什么

增量缓存的失效判据（`.meta` / `package.meta`）只 pin「源内容 SHA-256 + zbc/zpkg 格式 Minor」，
两者都测不出**「编译器语义变了但格式没变」**：codegen / 优化改动多数不动 wire 格式 → 格式 Minor 不 bump →
`ProbeFiles` 命中旧 cache → **静默复用旧 `.zbc` 产物**，产物与当前编译器语义不一致。
指纹补的就是这个失效次元。

## 它怎么算

**指纹 = `CompilerFingerprint.Entries` 这张列表的内容哈希**（`ZpkgBuilder.SourceHashHex`）。
每条语义变更**追加一行自己的 slug**，无需手工取号。

```z42
public static string[] Entries = new string[] {
    "baseline-41",
    "fingerprint-content-derived",
    // 新变更追加在末尾，一行一条
};
```

### 为什么用列表而不是手工计数器

手工 +1 的计数器有两个实测损害：

| 损害 | 说明 |
|---|---|
| **撞号 / 让号** | 号是「先合先得」，并行的 PR 取同一个号，后到的必须改自己的 PR |
| 🔴 **正文被整行覆盖而丢失** | 两个 PR 基于同一个号、改**同一行**，后合的整行胜出，**git 不报冲突**，先合那条的整条理由凭空消失 |

列表方案把两者结构性消掉：**没有号可抢**；两个 PR 各追加一行，合并只会**两行都留下**（要么自动合并，要么冲突时正解是「都留」而不是「谁让」）。

⚠️ **为什么不去哈希编译器全部源码**：实测那会让**每次注释编辑**都全量失效 ——
空跑 12.0s / 改一行注释 18.0s / 指纹变一次 **26.8s**（`cached: 0/123`），即内循环 +50%，
还连带 stdlib 全量。列表方案只在**人记录了一条语义变更**时才变，零额外失效。
（代价：仍需人判断「这次要不要记一条」—— 由 CI 守门兜底。）

## 什么时候要追加一条

| 情形 | 动作 |
|------|------|
| 改 codegen / 优化 pass / typecheck / lowering / **发出的诊断**，**且不 bump zbc/zpkg 格式** | 在 `CompilerFingerprint.Entries` **末尾追加一行本次变更的 slug** |
| bump 了 zbc/zpkg 格式 Minor | **不必**追加——格式 Minor 变化已让所有旧 `.meta` 失效（追加了也无害）|
| 只修 reader / writer 非格式 bug（不改 wire、不改编出的字节、不改诊断） | 不追加 |

判据一句话：**同一份源码的编译结果（含诊断集）是否可能变**。

> ⚠️ 第 1 行明确含「发出的诊断」。「发码没变」不是不必追加的理由——**诊断变了、字节没变**恰恰是最需要
> 失效的一档：那类源文件哈希一字未变，不失效就会命中旧条目、把新诊断整个吞掉。
>
> ⚠️ 第 3 行的主语是 **reader / writer**，不是「字节没变」——别把括号里的条件当成独立判据去和第 1 行对撞。

**坐标**：`src/compiler/z42c.pipeline/src/CompilerFingerprint.z42` 的 `Entries`（**只许在末尾追加**，
不许改动 / 删除既有条目）；`CacheStore.Fingerprint()` 只是它的取值口。它进 `.meta` 的 `z42c-fp` 行与
`package.meta` 头，`Parse` / `LoadSrcList` 校验不符即令条目作废。**纯 z42c 内部格式：不涉 wire、不触发
zbc/zpkg 格式 bump、不需改 Rust 端。**

### CI 守门：输出变了就必须累加

「该不该追加」由 CI 守门判断：bench-pr 工作流的 **Compiler fingerprint guard** 步骤用 base 编译器和
PR 编译器各编一遍**同一份 base stdlib 源码**，逐包比 zpkg 字节（同源同编译器 ⇒ 逐字节一致）。

| 输出字节 | 编译器身份（`Entries` / 格式 Minor） | 结果 |
|---------|-----------------------------------|------|
| 不变 | — | ✅ |
| 变了 | 至少一个变了 | ✅ |
| 变了 | 都没变 | ❌ 报出哪些包变了 → 去 `Entries` 末尾追加一行 slug |

> 🔴 **这道门对「诊断变了、发码不变」那一档是结构性地瞎的**——它比的是**产物字节**，而诊断不进产物。
> 那一档只能靠人按上表第 1 行记一条，门禁不会替你发现。「不变」一格并不等于纯重构：**只改诊断也落在这一格**。

本地复现：`xtask test compiler fingerprint --base <base 源码树根>`（base 树的 stdlib 须先由 base 编译器建好）。
覆盖面 = stdlib 实际走到的编译器路径；stdlib 没用到的 codegen 分支测不到（只会漏判，不会误判）——
这类改动仍按上表手动追加。
