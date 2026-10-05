# 编译器语义指纹（`CompilerFingerprint`）

> 页型：机制页 ｜ 代码：`src/compiler/z42c.pipeline/src/CompilerFingerprint.z42`
> 规则见 [version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md)「编译器语义指纹」。

## 它是什么

增量缓存的失效判据里，「源内容哈希 + zbc/zpkg 格式 Minor」测不出**「编译器语义变了但格式没变」**。
指纹补的就是这个次元：它进 `.meta` 的 `z42c-fp` 行与 `package.meta` 头，不符即令条目作废。

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

判据不变，见 [version-bumping.md](https://github.com/z42-lang/z42/blob/main/docs/agent/rules/version-bumping.md)。一句话：
**同一份源码的编译结果（含诊断集）是否可能变**。⚠️ 注意「诊断变、发码不变」也算 ——
那一档 **CI 的 fingerprint 守门是瞎的**（它比的是产物字节），只能靠人记。
