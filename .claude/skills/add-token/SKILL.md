---
name: add-token
description: 向 z42 词法器添加新的 token 类型。在用户说"添加 token"、"新增关键字"、"加符号" 等时触发。
user-invocable: true
allowed-tools: Read, Edit, Grep
argument-hint: <token-name> [keyword-text]
---

# 添加 Token：$ARGUMENTS

向 z42 词法器添加新 token，需要按顺序修改以下文件：

## 步骤 1 — TokenKind.cs

在 [TokenKind.cs](src/compiler/Z42.Compiler/Lexer/TokenKind.cs) 的合适分类中追加枚举成员：
- 关键字 → `// Identifiers & keywords` 区域
- 类型名 → `// Types` 区域
- 符号 → `// Symbols` 区域

## 步骤 2 — Lexer.cs

打开 [Lexer.cs](src/compiler/Z42.Compiler/Lexer/Lexer.cs)：

**如果是关键字**：在 `Keywords` 字典中添加一行：
```csharp
["keyword-text"] = TokenKind.NewKind,
```

**如果是符号**：在 `NextToken()` 的 switch 表达式中添加对应分支。
多字符符号（如 `->`, `=>`, `!=`）用 `Peek()` + `Eat()` 配合处理。

## 步骤 3 — 验证

确认修改后能通过编译：
```bash
xtask build compiler
```

用 `--dump-tokens` 验证新 token 的词法结果（写一个含新 token 的临时 .z42 文件）：
```bash
artifacts/build/runtime/release/z42vm artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg -- --dump-tokens /tmp/tok.z42
```
