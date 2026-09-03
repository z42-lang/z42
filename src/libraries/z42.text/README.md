# z42.text — 文本处理库

## 职责

z42 文本处理类型。**纯脚本实现** —— 严格遵循 [`src/libraries/README.md`](../README.md)
"VM 接口集中在 z42.core" 规则，本包不声明任何 `[Native(...)] extern` 方法。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `StringBuilder.z42` | `StringBuilder` | 字符串拼接缓冲区 — Script-First 实现，基于 `string[]` 收集片段，`ToString` 走 `String.ConcatParts` 单次原生拼接（perf-stdlib-hot-paths）。C# `System.Text.StringBuilder` 对齐：`Append`/`AppendLine`/`AppendFormat`、`Insert`/`Remove`/`Replace`/`Clear`、索引器 `this[int]`、`Length { get; set; }`、`GetLength`/`ToString` |
| `Levenshtein.z42`   | `Levenshtein` static class | 编辑距离 `Distance(a, b)` + 归一化相似度 `SimilarityRatio(a, b) ∈ [0,1]`（fuzzy search / 拼写纠错） |
| `Strings.z42`       | `Strings` static class | 字符串 shaping helpers：`PadLeft / PadRight / Repeat / IndexOfAny / TrimChars`（expand-z42-text-strings, 2026-06-03，review.md S3/S5 Phase 1） |

> **Regex 在 [`z42.regex`](../z42.regex/)** —— 不在本包。本包的 `Regex.z42` 旧 stub 已删除（commit 2026-05-24 docs/review.md Part 3 S2.2 清理）。

## 实现备注

`StringBuilder` 内部用 `string[]` 收集 Append 片段（按 2× 扩容），ToString 时
经 `String.ConcatParts(parts, count)` 一次原生拼接（perf-stdlib-hot-paths；此前是逐字符
`CharAt` 复制进 `char[]` 再 `FromChars`，每个输出字符一次 builtin 派发）。不用 `List<string>` 是因为
parser 当前对字段声明的泛型实例化语法（`List<string> _parts;`）会误识别为
method header；待后续 parser 修复后可以切换。

随机访问与就地编辑成员（索引器 `this[int]` 的 setter、`Length` setter、`Insert`/
`Remove`/`Replace`）不能直接落在分段 buffer 上，故先 `ToString()` 收敛成单段字符串
再 `_setSingle` 重置为一段，单次编辑 O(n)——契合 StringBuilder「大量 Append、偶发编辑」
的使用画像。索引器 getter 走分段遍历（O(段数)），不做 collapse。
