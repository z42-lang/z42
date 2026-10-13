# z42.doc

## 职责
`z42 doc` 的引擎：从源码的 doc 注释建符号索引，按名字查签名、文档与 `file:line`。命名空间 `Z42.Doc`。
语法级索引只解析不做语义分析（含私有成员），按文件增量缓存。命令行与打印在 z42d（`src/toolchain/devtools`）。

## 功能索引
| 功能 | 入口 |
|------|------|
| 一次查询（选项 → 输出文本 + 退出码） | `DocQuery.Run(DocQueryOptions)` |
| 源文件 → 条目 | `SyntaxIndexer.IndexSource(src, file)` |
| 名字解析 / 成员 / 搜索 | `DocIndex.Find` / `Members` / `Search` |
| 输出形态 | `DocFormat.Show` / `Outline` / `Candidates` / `SearchResults` / `Json` |
| 索引根与源文件发现 | `SourceTree.FindRoot` / `Collect` |
| 增量缓存 | `IndexCache.Build(root, files, cachePath)` |

## 如何测试验证
```bash
./xtask test stdlib z42.doc    # tests/index.z42（Doc ID / 签名 / 可见性 / 摘要）+ tests/query.z42（解析顺序 / 轮廓 / 缓存 / 示例工程）
```
`tests/fixtures/sample/` 是示例工程：演示 doc 注释写法，也是端到端查询的夹具。

## 关联文档
- 机制：[文档索引与 `z42 doc`](../../../docs/internals/src/toolchain/doc.md)
- 用法：[`z42 doc`](../../../docs/reference/src/toolchain/cli-z42.md)、[doc 注释](../../../docs/reference/src/language/doc-comments.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/DocItem.z42` | 索引条目（Doc ID / 种类 / 签名 / 位置 / 可见性 / doc 原文）与 `DocKind` |
| `src/SyntaxIndexer.z42` | AST → 条目：Doc ID、默认可见性、impl 方法并入目标类型 |
| `src/DocSig.z42` | 声明 → 单行签名（含默认值的源码写法） |
| `src/DocIndex.z42` | 条目集合与查询：四级名字解析、成员、搜索 |
| `src/DocFormat.z42` | 文本 / JSON 输出 |
| `src/SourceTree.z42` | 索引根定位、源文件遍历、按最近的清单归包 |
| `src/IndexCache.z42` | 增量缓存（按文件大小 + 修改时间失效） |
| `src/DocQuery.z42` | 查询编排 |

## 依赖关系
`z42c.core`、`z42c.syntax`。被 z42d（`src/toolchain/devtools/core`）以 path 依赖引用并复制进其程序目录。
