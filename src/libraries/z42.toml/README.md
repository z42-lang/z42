# z42.toml

## 职责
TOML 1.0 subset reader / writer。覆盖 `z42` manifest（`*.z42.toml`）和
build-driver `versions.toml` 解析需要的所有语法。

不实现：datetime、注释 / 原格式保留、Stringify 格式选项
（见 [docs/reference/src/stdlib/toml.md](../../../docs/reference/src/stdlib/toml.md)「不支持」节）。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/TomlValue.z42` | 值类型 + 公开入口（`Parse` / `Stringify` / `Of*` 工厂 / `Is*` / `Get` / `Set`） |
| `src/TomlException.z42` | `TomlException : Std.Exception`，带 1-based line / column |
| `src/TomlParser.z42` / `src/TomlParser.Numbers.z42` | 内部 tokenizer + recursive-descent parser（partial，数字字面量在 `.Numbers`） |
| `src/TomlWriter.z42` | 内部 stringifier（canonical TOML，sort 后输出） |

## 入口点
公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/toml.md)（由源码 doc 注释生成）。

## 用法

```z42
using Std.Toml;
using Std;

var t = TomlValue.Parse("name = \"foo\"\n[deps]\nz42.core = \"0.1.0\"");
t.Get("name").AsString();                     // "foo"
t.Get("deps").Get("z42.core").AsString();     // "0.1.0"

var root = TomlValue.OfTable();
root.Set("version", TomlValue.OfLong(42));
TomlValue.Stringify(root);                     // "version = 42\n"
```

## 依赖关系
依赖 `z42.core` + `z42.io`（`ParseStream` / `WriteTo` 的 Stream 重载）。

## 如何测试验证

```bash
xtask test stdlib z42.toml    # 本库全部 [Test]
```

## 关联文档

- 用法与规则（参考手册）：[z42.toml](../../../docs/reference/src/stdlib/toml.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)
