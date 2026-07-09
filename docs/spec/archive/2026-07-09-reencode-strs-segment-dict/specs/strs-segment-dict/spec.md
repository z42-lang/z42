# Spec: STRS segment-dict 编码

## MODIFIED Requirements

### Requirement: STRS 段 wire 布局

**Before:** STRS = `u32 count` + `count×(u32 offset, u32 len)` + 拼接 UTF-8 blob；
串 i 的字节 = `blob[offset[i] .. offset[i]+len[i]]`。offset 恒为 `Σ len[0..i]`（冗余）。

**After:** STRS = segment-dict：

```
u32     segCount
segCount × { varint segByteLen ; utf8 segBytes }        // 唯一段,first-seen 序,去重
u32     strCount
strCount × { varint segN ; segN × varint segIdx }       // 每串 = 段索引序列
```

串 i = `join(segDict[segIdx] for segIdx in string_i.seq, ".")`。`strCount` 顺序等于旧池
顺序，串索引 i 语义不变。

#### Scenario: 共享前缀去重往返
- **WHEN** 池含 `["Std.A", "Std.B", "Std.A.C"]`（intern 序）
- **THEN** 段字典去重为 `["Std","A","B","C"]`（segCount=4）；strTable =
  `[[0,1],[0,2],[0,1,3]]`；reader 还原回原三串，逐串相等

#### Scenario: 无点串与空串无损
- **WHEN** 池含 `["int", "", "3.14"]`
- **THEN** `"int"`→单段 `["int"]`；`""`→单空段 `[""]`；`"3.14"`→`["3","14"]`；
  reader join('.') 精确还原 `["int","","3.14"]`

#### Scenario: 串索引对消费端透明
- **WHEN** 某函数签名在 SIGS 用 `pool.Idx("Std.String")` 写下 u32 索引 k
- **THEN** 新编码下该函数仍写同一个 k（k = 该串在池中的位置，未变）；reader 用 k 查 STRS
  仍得 `"Std.String"`；SIGS/MODS/TSIG 等段字节零变化（除各自不含 STRS blob）

#### Scenario: 段序确定性（自举不动点）
- **WHEN** 同一 IrModule 由 gen1 与 gen2 z42c 分别 emit STRS
- **THEN** 段字典顺序、strTable、整段字节逐字节相等（段序纯由稳定池序派生）

#### Scenario: strict-pin 版本校验
- **WHEN** 旧 minor（zbc 1.20 / zpkg 0.24）的 reader 遇到新 minor 产物
- **THEN** 按既有 strict-pin 直接拒绝（无兼容回退）；新 reader 只读 1.21 / 0.25

#### Scenario: varint 越界保护
- **WHEN** STRS 中某 segIdx ≥ segCount，或 varint 读越过段边界
- **THEN** reader 报明确错误（Rust `bail!`，z42 诊断），不静默返回错串

## IR Mapping

无新 IR 指令。纯二进制格式（STRS 段编码）变更。

## Pipeline Steps

- [ ] Lexer — 不涉及
- [ ] Parser / AST — 不涉及
- [ ] TypeChecker — 不涉及
- [x] IR Codegen（zbc/zpkg writer：`ByteWriter.WriteVarint` + `ZbcWriter.BuildStrs`）
- [x] VM interp（Rust `read_strs` + `Cursor::read_varint`）
- [x] z42c reader（`ZbcReader._readStrs` + `ZpkgReader.Open` STRS + 两游标 `Varint`）
