# Spec: 字符串/字符转义序列校验

## ADDED Requirements

### Requirement: 完整的标准单字符转义集

z42 字符串字面量、字符字面量、插值串文本段识别以下单字符转义，解码为对应字符：

| 转义 | 字符 | 码点 |
|------|------|------|
| `\a` | 响铃 | 0x07 |
| `\b` | 退格 | 0x08 |
| `\f` | 换页 | 0x0C |
| `\n` | 换行 | 0x0A |
| `\r` | 回车 | 0x0D |
| `\t` | 制表 | 0x09 |
| `\v` | 垂直制表 | 0x0B |
| `\0` | 空字符 | 0x00 |
| `\\` | 反斜杠 | 0x5C |
| `\"` | 双引号 | 0x22 |
| `\'` | 单引号 | 0x27 |

#### Scenario: 新增控制字符转义解码正确
- **WHEN** 源码含 `"\b"` / `"\f"` / `"\a"` / `"\v"`
- **THEN** 解码得到码点 0x08 / 0x0C / 0x07 / 0x0B 的单字符字符串（不再是字母 `b`/`f`/`a`/`v`）

#### Scenario: char 字面量控制转义
- **WHEN** 源码含 `'\b'`（JSON/TOML 解析器已在用）
- **THEN** 该 char 值为 0x08，`'\b'` 不再被解成字母 `b`

### Requirement: 未知转义序列报错 E0102

字符串/字符/插值串文本段中出现不在合法集内的 `\X`（如 `\U` `\D` `\q` `\p` `\u` `\x`），词法器发出诊断 `E0102 InvalidEscape`，消息面向使用者，指明具体非法转义。

#### Scenario: 未知转义在普通字符串中报错（回归本 bug）
- **WHEN** 源码含 `"C:\Users\bin"`（`\U` `\b`… 中 `\U` 非法）
- **THEN** 报 `E0102`，消息形如 `unrecognized escape sequence '\U'`，指向该 `\U` 的 span
- **AND** 不再静默丢反斜杠产出 `C:Usersbin` 类损坏字符串

#### Scenario: 未知转义在 char 字面量中报错
- **WHEN** 源码含 `'\q'`
- **THEN** 报 `E0102`

#### Scenario: 数字/Unicode 转义暂不支持 → 报 E0102（Deferred，诚实报错）
- **WHEN** 源码含 `"é"` 或 `"\xFF"`
- **THEN** 报 `E0102`（而非静默产出 `u00e9`）；`\uXXXX`/`\xXX` 的支持列入 Deferred

### Requirement: 逃生舱不受影响

#### Scenario: raw 串逐字保留反斜杠
- **WHEN** 源码用 raw 串 `"""C:\Users\bin"""`
- **THEN** 内容逐字保留（含反斜杠），不做转义解码，不报 E0102

#### Scenario: 转义反斜杠仍合法
- **WHEN** 源码含 `"C:\\Users\\bin"`
- **THEN** 正常解码为 `C:\Users\bin`，无诊断

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更。转义解码是纯前端（Lexer/Parser）行为，字符串字面量最终 emit 为既有 `StrConst`。

## Pipeline Steps

受影响的 pipeline 阶段：
- [x] Lexer（新增转义校验 + 补全 DecodeString 映射）
- [ ] Parser / AST（不变，仍复用 DecodeString）
- [ ] TypeChecker（不变）
- [ ] IR Codegen（不变）
- [ ] VM interp（不变）
