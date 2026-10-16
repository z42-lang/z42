# z42.text — 文本处理库

## 职责

z42 文本处理类型。**纯脚本实现** —— 严格遵循 [`src/libraries/README.md`](../README.md)
"VM 接口集中在 z42.core" 规则，本包不声明任何 `[Native(...)] extern` 方法。

## 入口点

公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/text.md)（由源码 doc 注释生成）。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `StringBuilder.z42` | `StringBuilder` | 可变字符串缓冲（Script-First，`string[]` 收集片段 + `String.ConcatParts` 单次原生拼接） |
| `Levenshtein.z42`   | `Levenshtein` static class | 编辑距离 + 归一化相似度（fuzzy search / 拼写纠错） |
| `Strings.z42`       | `Strings` static class | 字符串 shaping 静态工具 |

> 正则在 [`z42.regex`](../z42.regex/)，不在本包。

## 实现备注

`StringBuilder` 内部用 `string[]` 收集 Append 片段（按 2× 扩容），ToString 时
经 `String.ConcatParts(parts, count)` 一次原生拼接。不用 `List<string>` 是因为
parser 当前对字段声明的泛型实例化语法（`List<string> _parts;`）会误识别为
method header；待后续 parser 修复后可以切换。

随机访问与就地编辑成员（索引器 `this[int]` 的 setter、`Length` setter、`Insert`/
`Remove`/`Replace`）不能直接落在分段 buffer 上，故先 `ToString()` 收敛成单段字符串
再 `_setSingle` 重置为一段，单次编辑 O(n)——契合 StringBuilder「大量 Append、偶发编辑」
的使用画像。索引器 getter 走分段遍历（O(段数)），不做 collapse。

## 如何测试验证

```bash
xtask test stdlib z42.text    # 本库全部 [Test]
```

## 关联文档

- 用法与规则（参考手册）：[z42.text](../../../docs/reference/src/stdlib/text.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)

## 依赖关系
仅依赖 `z42.core`。
