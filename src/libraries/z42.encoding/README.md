# z42.encoding

## 职责

byte ↔ text 编码三件套：Hex / Base64 / UTF-8。脚本实现（本包零 extern，UTF-8 复用 core 的字符串存储原语）。

## 核心文件

| 文件 | 职责 |
|------|------|
| `src/Hex.z42` | `Std.Encoding.Hex` — Encode / EncodeUpper / Decode |
| `src/Base64.z42` | `Std.Encoding.Base64` — RFC 4648 §4 标准 Base64（含 `=` padding） |
| `src/Base64Url.z42` | `Std.Encoding.Base64Url` — RFC 4648 §5 URL-safe Base64 |
| `src/Base32.z42` / `src/Base32Hex.z42` / `src/Base32Crockford.z42` | Base32 三个字母表变体（RFC 4648 标准 / Extended Hex / Crockford） |
| `src/Utf8.z42` | `Std.Encoding.Utf8` — GetBytes / GetString，复用 String.CopyUtf8To / FromUtf8，严格校验 UTF-8 |
| `src/Utf16.z42` | `Std.Encoding.Utf16` — `GetBytesLE/BE` + `GetStringLE/BE`；surrogate pair + 严格校验 |
| `src/Utf32.z42` | `Std.Encoding.Utf32` — `GetBytesLE/BE` + `GetStringLE/BE`；定宽 4 bytes/codepoint，拒 surrogate / >U+10FFFF |
| `src/Encoding.z42` | `Std.Encoding.Encoding` — 编码对象（目前仅 UTF-8，`StreamReader` / `StreamWriter` 按它编解码） |

## 入口点
公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/encoding.md)（由源码 doc 注释生成）。

## 依赖关系

→ `z42.core`（byte / char / string / Exception / FormatException）。本包零 extern；UTF-8 经 core 的存储复制/严格字符串构造实现。

## 错误处理

非法输入抛 `Std.FormatException`：
- Hex.Decode 奇数长度 / 非法字符
- Base64.Decode 非法字符 / 长度错误 / 内部 padding
- Utf8.GetString 截断 / overlong / surrogate / 超界 / 非法首字节

## 待办
- 无 streaming API（Encoder / Decoder 状态机）

## 关联文档

- 用法与规则（参考手册）：[z42.encoding](../../../docs/reference/src/stdlib/encoding.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)
