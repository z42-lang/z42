# Spec: Script-First 字符串搜索（scalar IndexOf over char[]）

## ADDED Requirements

### Requirement: `__str_to_chars` bulk view 原语产 Unicode scalar

#### Scenario: 多字节 UTF-8 产 scalar 序列
- **WHEN** `"héllo".ToCharArray()`（`é` 占 2 UTF-8 字节）
- **THEN** 得 5 个 scalar `['h','é','l','l','o']`（非 6 字节）

#### Scenario: 空串
- **WHEN** `"".ToCharArray()`
- **THEN** 空 char[]

### Requirement: `IndexOf` 返回 scalar 索引（char[] 脚本实现）

#### Scenario: ASCII 命中 / 未命中 / 空 / 超长
- **WHEN** `"abcmnop".IndexOf("mnop")` / `"abc".IndexOf("xyz")` / `"abc".IndexOf("")` / `"ab".IndexOf("abc")`
- **THEN** `3` / `-1` / `0` / `-1`

#### Scenario: UTF-8 —— scalar 索引而非字节偏移
- **WHEN** `"héllo".IndexOf("llo")` / `"日本語テスト".IndexOf("テスト")`
- **THEN** `2` / `3`（scalar 索引，与 `CharAt`/`Length` 自洽；byte offset 分别是 3 / 9）

#### Scenario: Contains 自动获益
- **WHEN** `"hello".Contains("ell")` / `"hello".Contains("zzz")`
- **THEN** `true` / `false`（`IndexOf(value) >= 0`，语义不变）

## MODIFIED Requirements

### Requirement: `String.IndexOf` / `ToCharArray` 实现改为 char[] 脚本 + bulk 原语

**Before:** `IndexOf` 逐字符 `CharAt`（builtin 派发）O(n·m) 脚本扫描；`ToCharArray` 逐字符 `CharAt` 物化。
**After:** `ToCharArray` = `[Native("__str_to_chars")]` 一次 bulk 物化；`IndexOf` 在**脚本**里 over 该
char[]（`arr[i]` = ArrayGet opcode）扫描。**对外行为（scalar 语义）逐字不变**，实测 interp 8.6× 提速。

#### Scenario: 与旧逐字对齐
- **WHEN** 任意 (haystack, needle)（ASCII / UTF-8 / 边界）
- **THEN** 新 IndexOf 结果 == 旧 CharAt 版结果

#### Scenario: z42c 自举字节不动点
- **WHEN** 用改后源自建 z42c（gen1 → gen2）
- **THEN** gen1==gen2（行为不变；两代同源）

## IR Mapping
- 无新 IR 指令。新增 corelib builtin 名 `__str_to_chars`（复用 `Builtin` 指令）。zbc 格式不 bump。

## Pipeline Steps
- [ ] Lexer / Parser / TypeChecker：无
- [x] VM corelib：新增 + 注册 `__str_to_chars`
- [x] stdlib：ToCharArray→bulk、IndexOf→char[] 脚本
