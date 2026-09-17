# design/language —— 搬迁中（冻结只读）

> ⛔ **本目录冻结只读。** 要改其中任何一篇，**必须先按搬迁清单把它搬进目标书，再在新位置改**——
> 不允许原地修改。规则见 `docs/agent/rules/doc-system.md` 的过渡期节。

语言参考已迁入[参考手册](https://z42-lang.github.io/z42/reference/)，实现机制迁入[实现内幕](https://z42-lang.github.io/z42/internals/)。

## 尚未搬迁

| 文件 | 去向 | 批 |
|---|---|---|
| `interop.md` | §1 §3 §4 §5.1 §6 §7.2-7.3 §8.4 的 C ABI 契约 → reference/embedding；三层架构 / 调用约定 / 内存 / manifest / §11 L1 `[Native]` → internals | 3b |
| `object-protocol.md` | 契约 → reference；派发实现 → internals | 3b |
| `boxing.md` | 语义 → reference 的 `conversions.md`；插入点实现 → internals 的 `struct-value-semantics.md` | 3b |
| `closure.md` | 语法与捕获语义 → reference；三档实现 → internals 的 `escape-analysis.md` | 3b |
| `attributes.md` | 用法 / API / `#suppress` / caller 宏 → reference；工厂 thunk / 元数据持久化 / registry → internals | 3b |
| `reflection.md` | → reference 的 stdlib 部分 | 4 |
| `string-builtins.md` | → reference 的 stdlib 部分（`string.md`） | 4 |

搬迁清单与裁决见 `docs/spec/changes/restructure-docs-three-books/`。
