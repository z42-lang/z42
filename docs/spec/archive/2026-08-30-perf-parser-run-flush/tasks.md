# Tasks: 解析器 run-flush + StringBuilder 快路径 + BigInt 尾填

> 状态：🟢 已完成 | 创建：2026-08-30 | 完成：2026-08-30 | 类型：perf（最小化模式）

**变更说明：** library_review.md 性能 P1/P2/P3 —— ①6 处 JSON/TOML/YAML 引号字符串解析改
run-flush（普通字符成段 Substring 冲刷，分配 O(字符数)→O(转义数)）；②StringBuilder 加
`Append(string)`/`Append(char)` 重载（避开 `Append(object)` 的 Convert+装箱）；③BigInt
ToString/ToBase/ToHex 的 O(n²) 字符串拼接改 char[] 一次 FromChars。

**原因：** 写入侧（JsonWriter.QuoteString）已用 run-flush，解析侧漏了、不对称；StringBuilder 缺
string/char 重载使 run-flush 的段仍走装箱；BigInt 十进制/hex 化每位一拼是位数的 O(n²)。

**文档影响：** StringBuilder README（功能索引加 Append 重载，若有）；其余为内部实现优化，行为不变，
归档 spec 记录即可。

**设计校正（对 review P2）：** review 说「StringBuilder 无 char 缓冲是 P1 根因，应增设 char[] 缓冲」——
对 run-flush 场景是反的：run-flush 把普通字符批成**字符串 run**（Substring）再 Append，即 Append 的是
字符串。对「append 整串」，现有 `string[] _parts`（存引用、ToString 时才一次拷贝）**最优**；改 char[]
缓冲反而把每个 run 拷两遍、回退收益。故只加 `Append(string)`（快路径）+`Append(char)`、保留 string[]。

- [x] 1.1 StringBuilder：加 `Append(string)`（直存 _parts、跳 Convert）+ `Append(char)` + `Append(object)` 委托到 string 路径（z42.text）
- [x] 1.2 StringBuilder 测试：char / string+char 混合 / string 重载与 object 一致 / 非字符串仍走 Convert（5 个 [Test]）
- [x] 2.1 JSON JsonParser.ParseString run-flush（z42.json）
- [x] 2.2 TOML ParseBasicString / ParseLiteralString / ParseMultiLineBasicString / ParseMultiLineLiteralString run-flush（z42.toml，4 处，含 fold/闭合三引号 runStart 更新）
- [x] 2.3 YAML _ParseDoubleQuotedString / _ParseSingleQuotedString run-flush（z42.yaml，2 处）
- [x] 3.1 BigInt ToString / ToBase / ToHex：`s = s + 片段` O(n²) → char[] 一次 FromChars（z42.numerics）
- [x] 4.1 targeted 验证：z42.text/json/toml/yaml/numerics 全绿（含多行 TOML fold + BigInt base 交叉验证）
- [x] 4.2 完整 GREEN gate（`xtask test` 全 stage 通过：e2e / cross-zpkg / 全 stdlib / 编译器自举 gen1==gen2 / vscode-syntax。注：首跑 text.app e2e 因 stale `/tmp/z42c-e2e-*` 假失败，清 tmp 后全绿）
- [x] 4.3 归档 + PR

## 备注
- 纯 stdlib 实现优化，行为不变（现有测试全绿即回归保证）；无新语言特性/无格式 bump/无 bootstrap 越界。
- BigInt 用 char[]+FromChars 而非 StringBuilder，避免给 z42.numerics 新增 z42.text 依赖。
- \b/\f/\0 等 1-char 转义顺带改用新 `Append(char)` 重载（去掉手搭 char[]+FromChars）。
