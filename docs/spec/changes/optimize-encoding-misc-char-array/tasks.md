# Tasks: optimize-encoding-misc-char-array

> 状态：🟢 已完成 | 创建：2026-08-27
> 类型：perf / refactor（最小化模式）；属「stdlib 性能改善程序」PR4

**变更说明：** encoding / text / numerics / io 里对长输入串的逐字符 `CharAt` 读取，改为
一次 `ToCharArray()` 物化 + O(1) 数组索引。
**原因：** 同 PR2——`CharAt` 每次走 builtin 派发；热路径逐字符扫描时常数因子显著。
**文档影响：** 无——纯内部机制替换，行为字节等价，由现有测试守住。

## 覆盖
- [x] 1.1 `z42.text/Levenshtein.z42`：DP 内层循环读 `s[k-1]` 是 O(ls·lt) 次——物化 `sarr`/`tarr`
- [x] 1.2 `z42.io/StringReader.z42`：字段 `_source` → 加 `char[] _chars` 物化，Peek/Read/ReadLine 5 处改索引；Close 同步清 `_chars`
- [x] 1.3 `z42.encoding/Base64.z42` `Decode`：输入 `s` → `char[] cs`（10 处走串读取）
- [x] 1.4 `z42.encoding/Hex.z42` `Decode`：输入 `hex` → `char[] hc` + 巻出 `hex.Length/2`
- [x] 1.5 `z42.numerics/BigInt.z42` `Parse`：输入 `s` → `char[] cs`（数字循环）
- [x] 2.1 bench：`encoding_bench.z42` 加 base64/hex 4K round-trip（KB 级，测得改进）
- [ ] 3.1 GREEN 交 PR CI（本机 z42vm wedge）；字节等价由现有 encoding/text/numerics/io 测试守住

## 本次未做（follow-up，风险/收益权衡）
- **encode 侧 `alpha.CharAt(6-bit/nibble)` 查表**：`_alpha` 是 64/16 字符短串，CharAt 已很便宜
  （ASCII 字节索引），转 char[] 收益微 → 不动。
- **Base32 / Base32Hex / Base32Crockford**：同 decode 模式，盲改风险，留后续（Base64/Hex 覆盖最高频）。
- **BigInt.ParseHex**：同 Parse 模式，十进制 Parse 更常用，先只做 Parse。

## 备注
- 零格式 bump。行为字节等价（无新增测试；现有 encoding/text/numerics/io 测试套件覆盖）。
