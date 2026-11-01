# 扩展语言与 IR：新增词法元素 / 语法 / IR 指令

> 页型：操作页 ｜ 代码：`src/compiler/z42c.syntax/`、`src/compiler/z42.package/`、`src/runtime/`
> 相关：[架构总览](architecture.md) · [源代码编译流程](source-compile.md) ·
> [格式 bump：清单与本地验证](../devinfra/format-bump.md) · [IR 指令集](../formats/ir.md)

本页回答「这件事要改哪些文件、按什么顺序」。语言与 IR 的**机制**见 [source-compile.md](source-compile.md)；
改了格式之后**在本地怎么重建与重生 fixture** 见 [格式 bump](../devinfra/format-bump.md)。

## 一、新增词法元素 → 改哪里

| 新增什么 | 改的文件 | 加什么 |
|----------|---------|--------|
| 新关键字 | `TokenKind.z42` + `Lexer.z42` | ① 新 `int` 常量；② `_initKeywords()` 加一行 `this._kw("name", TokenKind.Name);` |
| 新符号 / 运算符 | `TokenKind.z42` + `Lexer.z42` | ① 新 `int` 常量；② `_lexSymbol()` 的最长匹配分支加判别 |
| 新数字格式（如八进制） | `Lexer.z42` | `_lexNumber()` 在 hex/bin 判别后加分支 |
| 新数字后缀 | `Lexer.z42` | 扩 `_lexNumber()` 后缀消费逻辑 |
| 新字符串前缀 | `Lexer.z42` | `_lexOne()` 字符串分派前加前缀检测（参考 `_lexInterpolated` `$"..."`） |

> ⚠️ 关键字**追加在表尾**，不要按 `TokenKind` 数值区间判「是不是关键字」——区间判定会漏新关键字
> （`Lexer.IsKeywordKind` 是唯一判据，见 [source-compile.md](source-compile.md) 的三处「唯一真相」）。

## 二、新增语法 → 改哪里

| 新增什么 | 改的文件 | 加什么 |
|----------|---------|--------|
| 新表达式节点 | `Ast.z42` + `Parser.z42` | ① `sealed class XxxExpr : Expr`（带 `Span` + `Dump()`）；② `_parseExpr` 前缀 / 后缀分支分派 |
| 新语句 | `Stmt.z42` + `Parser.z42` | ① `sealed class XxxStmt : Stmt`；② `ParseStatement()` 加分派 |
| 新声明 | `Decl.z42` + `Parser.z42` | ① `sealed class XxxDecl : Decl`；② 顶层 / 成员解析加分派 |
| 新类型形式 | `TypeExpr.z42` + `Parser.z42` | ① `sealed class XxxType : TypeExpr`；② `_parseType()` 加分支 |

- **运算符优先级**：数值与跨文件依赖**只写在** `ParseTable.z42` 的 `ParseTable.LeftBp()` 及其头注释里
  （数值越大越紧；左结合，右操作数用 `bp + 1` 递归）。改优先级 = 编辑 `LeftBp()` 对应分支并同步它的头注释。
- **错误处理约定**：Parser **不抛异常**，遇错调 `_diags.Error(code, msg, span)` 累积后继续；错误码集中在
  `z42c.core/src/DiagnosticCodes.z42`；消息面向语言使用者，不暴露内部枚举名。恢复路径的四条规则
  （语句级 panic / 同行残渣 / 同位置去重 / 未闭合字符串）见 [source-compile.md](source-compile.md)。
- 新语法要落到 z42c 自身源码之前，先读下面「四、自举纪律」。

## 三、新增 IR 指令（opcode）

> ⚠️ **先读这一节：多数情况下你不该加 opcode。** 新增 IR opcode 要改 **~20 个文件**，并**必然 bump
> zbc + zpkg 格式**（strict-pin：minor 不等直接 bail），连带 zbc 6 个 + zpkg 4 个 fixture regen
> （后者无一键 regen）、golden hex 单测重截、触发 ci-bootstrap 两代自举。

**先评估这两条更便宜的路**：

| 路线 | 成本 | 何时适用 |
|---|---|---|
| **复用 `BuiltinInstr` + 常量字符串参数** | ~3 文件，**零格式 bump** | 新语义能表达成「一次 builtin 调用 + 常量串区分」。`BuiltinId` 不进 wire（是进程内下标），表尾追加即可。先例：`__box_prim`（`TypeOpEmitter._emitBox`）、`__sym_available`（`_emitSymAvailable`） |
| **attr-ref 哨兵 + 编译期消解** | ~5 文件，**零格式 bump** | 新特性能在编译期完全消解成既有构造。骑既有 SIGS/param attr-ref blob 通道，未标注符号不追加 → 老 golden 逐字节不变。先例：`$Deprecated` / `$Default` / `$Caller:*` |

同类需求优先选哨兵而非新 flag 位 / opcode，以避开格式-bump 两代自举回归。**拿不准就先问，别默认加 opcode。**

### 真要加 opcode：完整清单

**编译器侧**（`z42.package` + `z42c.optimization` + `z42c.emission`）：

1. `src/compiler/z42.package/src/IrInstr*.z42` — 新 `sealed class XxxInstr : IrInstr`
2. `src/compiler/z42.package/src/BinaryFormat/ZbcFormat.z42`（`static class Op`）— 新 opcode 常量
3. `.../ZbcInstr.z42` — 编码分支（带字符串则同时补 `InternInstrStrings`）
4. `.../ZbcReaderInstr.z42` — 解码分支 + 寄存器上界 / 重映射
5. `src/compiler/z42c.optimization/src/IrOptInfo.z42` — **4 处**：Dst / args 计数 / args 替换 / Dst 改写
6. `src/compiler/z42c.optimization/src/IrEscapeAnalysis.z42` — args 逃逸标记
7. 发射点（`src/compiler/z42c.emission/src/Emission/` 各 `*Emitter.z42`）

**运行时侧**（Rust）：

8. `src/runtime/src/metadata/ir/instruction.rs` — `Instruction` variant + `written_reg()`
9. `src/runtime/src/metadata/reader/opcodes.rs` — `OP_XXX` 常量
10. `src/runtime/src/metadata/reader/instr_decode.rs` — 解码
11. `src/runtime/src/metadata/resolver.rs` — 若需 per-site token 解析
12. `src/runtime/src/interp/exec_*.rs` — 执行语义。**`exec_instr` 的 match 无 `_` 兜底，必须显式处理**
13. `src/runtime/src/jit/translate/*.rs` — **要么翻译，要么**登记进
    `src/runtime/src/jit/translate/unsupported.rs` 的 `unsupported_reason`（单一真相表）

**格式 bump（强制链）**：新增 opcode **必然** bump zbc + zpkg。完整 9 步同步清单是
[格式 bump：清单与本地验证](../devinfra/format-bump.md) 的 §一（唯一 SoT）——**本页不复列**。

**文档**：IR 参考页（[formats/ir.md](../formats/ir.md)）、`src/compiler/z42.package/README.md`、
`src/runtime/src/interp/README.md`。

### 验证

```bash
cd src/runtime && cargo build --release --bin z42vm
./xtask build compiler
./xtask test                            # 全 stage
./xtask test compiler bootstrap         # 改了语法 / 格式能力必跑
```

## 四、自举纪律（新语法 / 新格式）

z42c 自身用 z42 写、由**上一个已发布 nightly 的 z42c** 编译，因此**新语法 / 新 zbc·zpkg 格式必须
support 先行、晚一个 nightly 再 use**——否则跨版本自举断链。完整纪律与 `xtask test compiler bootstrap`
边界检查见 [编译器自举](self-hosting.md)。

特性开关在 `z42c.core/src/LanguageFeatures.z42`（`Set(name, on)` / `IsEnabled(name)`）；需要按 phase
门禁某语法时在 Parser 查 `IsEnabled` 并报 feature-disabled 诊断。
