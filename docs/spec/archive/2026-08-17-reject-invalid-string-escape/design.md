# Design: 拒绝未知字符串转义 + 补全标准转义集

## Architecture

```
源码 "…\X…"
   │
   ▼
Lexer._lexString / _lexChar / _lexInterpolated
   │   逐字符扫描；遇 '\' + next：
   │   ├─ next ∈ 合法集 → 跳过（照旧）
   │   └─ next ∉ 合法集 → _diags.Error(E0102, span)   ← 新增校验
   ▼
Token(raw lexeme)  ─────────────────────────────►  ExprParser
                                                       │
                                                       ▼
                                            Lexer.DecodeString(raw)
                                              纯映射：合法集 → 目标字符
                                              （补 \a \b \f \v）      ← 新增映射
```

**职责切分**：
- **词法器**是转义合法性的**唯一裁决点**（emit E0102）——它已逐字符走、持 `_diags` + span，且 E0102 属 E01xx Lexer 码。
- **`DecodeString`** 是纯解码器：对已被词法器校验过的合法串做映射。它的 else 兜底分支对合法程序不再可达（保留为防御性，不再是"正常路径"）。
- 合法转义集在两处出现（词法器校验谓词 + DecodeString 映射），用一个共享静态谓词 `_isKnownEscape(char)` 收敛，避免两处漂移。

## Decisions

### Decision 1: 未知转义用 error 而非 warning
**问题：** 静默降级要换成 error 还是 warning？
**选项：** A — error（阻断编译，对齐 C# CS1009）；B — warning（放行但提示）。
**决定：** 选 A。根因是"静默数据损坏"，warning 仍会让损坏的字符串进入运行期。error 逼用户显式用 `\\` 或 raw 串，物理消除损坏路径。pre-1.0 无兼容包袱，直接切干净。

### Decision 2: 补全到 C# 单字符全集，而非只补 JSON/TOML 需要的 `\b \f`
**问题：** 只补 `\b \f`（修 bug 的最小集）还是补全 `\a \b \f \v`？
**选项：** A — 只 `\b \f`；B — `\a \b \f \v` 全补。
**决定：** 选 B。成本几乎为零（多两个映射分支），换来与 C# 单字符转义集完全一致、规则统一、无"为什么支持 \b 不支持 \v"的困惑。`\a \v` 虽罕用但属标准集。

### Decision 3: DecodeString 中 `\b`/`\f` 等控制字符的**自举安全构造**
**问题：** `DecodeString`（z42 源码）要产出退格 0x08。若直接写 `result + "\b"`，这段源码由**上一个 nightly z42c** 编译，而它的 DecodeString 还不认 `\b` → 会把 `"\b"` 解成 `b` → 自举鸡蛋问题（用要实现的特性去实现该特性）。
**选项：** A — 用码点构造控制字符（不依赖 `\b` 字面量）；B — 分两个 nightly（support 先行）。
**决定：** 选 A（一个 nightly 内自洽，无需分阶段）。DecodeString 内用**码点→char** 构造：以整数码点（8/12/7/11）建 char，再拼进结果（参照 JsonParser 已有写法 `char[] z = new char[1]; z[0] = <char>`，但 char 来自码点而非 `'\b'` 字面量——因为 `'\b'` 字面量同样受鸡蛋问题影响）。具体码点→char 的构造 API 在实施时确认（cast `(char)8` 或 stdlib char-from-int）。
> 关键：**新 z42c 源码本身不使用 `\b`/`\f`/`\a`/`\v` 字面量**，只*支持*它们。故上一 nightly 能编当前源，无跨版本断链，无需走两-nightly 纪律（bootstrap-seed.md 的 support/use 分离在此天然满足——本 change 只落 support，repo 内无 use）。

### Decision 4: JSON/TOML 源不改，行为随 DecodeString 自动修正
**问题：** JsonParser/TomlParser 里的 `'\b'`/`'\f'` 要不要改？
**决定：** 不改。它们的意图本就是"退格/换页"，只是被旧 DecodeString 误解成字母。补全映射后 `'\b'` 正确解为 0x08，这两个库的 `\b`/`\f` 转义解析**自动变正确**。`'\b'` 仍是合法词素（在新合法集内），不触发 E0102，不破坏编译。这是根因修复（改产出端 DecodeString）而非在 JSON/TOML 里打补丁。

### Decision 5: 修复 lexer 诊断端到端被丢弃（实施期根因，User 批准扩 Scope）
**问题：** 实施后端到端测试发现 `"C:\Users\bin"` **编译干净通过、E0102 不报**。根因：`Parser` 构造时 `new Lexer + Tokenize()`，但用**自己新建的** `_diags`，**从不并入** `_lx.Diagnostics()`；管线只消费 `Parser._diags`（经 `cu.ParseDiags`）。→ 所有 lexer 诊断（E0101 未终止串 / E0102 转义 / E0103 非法数字）**一直**端到端被丢（pre-existing latent bug），我的 E0102 只在 lexer 单测可见。
**选项：** A — ctor 里 pre-parse 合并（简单，但 lexer 错会让 `HasErrors()` 提前为真 → 破坏 [Parser.z42:66/90] 的 `if(!HasErrors()) MarkIncompleteAtEof()`，REPL 未终止串不再续行）；B — 收尾合并（merge-once 守卫，晚于 incomplete 判定）。
**决定：** 选 B。加 `_ensureLexDiagsMerged()`（`_lexMerged` 守卫，幂等）：
- parse 期的 incomplete 判定用 `_diags` **字段直读**，不经 `Diagnostics()` 访问器 → 不受合并影响，REPL 续行语义**不变**（`Completeness.IsIncomplete` 只读 incomplete **标志**，与 error 条目正交）。
- 在 `ParseCompilationUnit` 收尾（管线路径，`cu.ParseDiags` 同对象）+ `Diagnostics()` 访问器（REPL/其它消费者路径）各触发一次，守卫保证只合并一次。
- `DiagnosticBag.MergeFrom` 只追加 items、不复制 incomplete 标志。
**副作用（正向）：** E0101 未终止串、E0103 非法数字 也一并从"端到端不报"修好。
**验证：** REPL 不在 GREEN gate → 手动验 `"C:\bad"` 报 E0102 且 `"abc`(未闭合) 仍续行。

## Implementation Notes

- **合法集谓词**：`_isKnownEscape(char e)` → `e ∈ {a,b,f,n,r,t,v,'0','\\','"','\''}`。词法器三处（`_lexString`/`_lexChar`/`_lexInterpolated` 文本段）调用它；DecodeString 的分支与之一一对应。
- **span 精度**：E0102 的 span 指向 `\X` 两字符（`_curSpan` 从 `\` 起，长度 2），便于编辑器精确标注。
- **插值串**：`_lexInterpolated` 文本段（非 `{expr}` 洞）同样校验；洞内是普通表达式，其中的字符串字面量由递归的字符串规则覆盖。
- **`_lexChar` 目前不校验**：现在只 skip；新增非法转义分支。
- **不改二进制格式**：无 zbc/zpkg version bump。

## Testing Strategy

- **单元/golden（新）** `z42c.syntax/tests/lex-invalid-escape/`：
  - 正例：`"\b\f\a\v\n\t\r\0\\\"\'"` 解码为对应码点串（断言各字符码点）。
  - 反例：`"\U"` / `'\q'` / `"é"` → 断言产出 E0102（用 `coll.Diags` 直接断言 code，注意 `SemanticDump.ErrorCount` 可能不覆盖 lexer diags——按 memory `semanticdump-errorcount-skips-collector-diags` 的教训直接读词法诊断）。
- **回归**：JSON/TOML 若有 `\b`/`\f` 端到端用例，验证现在产出控制字符（若此前 expected 写的是字母，更新为正确值——属修 bug）。
- **GREEN**：`xtask test`（重点 `test compiler` 自举 5/5 + `test stdlib` JSON/TOML + `test e2e`）。self-host 字节：JsonParser/TomlParser 的 `'\b'`/`'\f'` 解码变化会改这两个库的 zbc 字节，但 gen1==gen2 仍成立（两代都用新 z42c）；golden `.zbc` 基线由 regen 波重生。

## Deferred / Future Work

### escape-future-numeric-unicode: 数字与 Unicode 转义
- **来源**：本 change（`Lexer.z42:9` 早已记 Deferred）
- **触发原因**：`\uXXXX` / `\xXX` / `\0` 八进制扩展 / `\UXXXXXXXX` 需要 hex/oct 解析 + 码点校验 + 越界诊断，独立工作量，与"拒绝未知转义"正交。
- **前置依赖**：无（可随时做）。
- **触发条件**：用户需要在字符串里写非 ASCII/控制字符的码点转义时。
- **当前 workaround**：本 change 后这些转义**报 E0102**（诚实告知暂不支持）；用户可直接在源码写 UTF-8 字面字符，或用 raw 串。
