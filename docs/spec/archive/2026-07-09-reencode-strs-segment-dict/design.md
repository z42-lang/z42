# Design: STRS segment-dict 重编码

## Architecture

STRS 段是 `.zbc` 与 `.zpkg` 共享的字符串池，单一 writer（`ZbcWriter.BuildStrs`）+ 三个
reader（z42 `ZbcReader._readStrs`、z42 `ZpkgReader.Open`、Rust `read_strs`）。本变更只
换 STRS **段内部**的字节编码，不动「其它段用 u32 池索引引用串」这一契约。

```
旧 STRS:                          新 STRS (segment-dict):
  u32 count                        u32 segCount
  (u32 offset, u32 len)×count      (varint segLen + utf8 seg)×segCount   ← 唯一段,去重,first-seen 序
  utf8-blob                        u32 strCount
                                   (varint segN + varint segIdx×segN)×strCount
  string[i] = blob[off..off+len]   string[i] = segDict[seq[0]] + "." + ... + segDict[seq[k]]

不变: strCount 顺序 == 旧 pool.At(i) 顺序 → 串索引 i 语义不变 → 消费端零改动
```

## Decisions

### D1: 段切分用 `.`，重组 join('.')——无损

**问题**：如何把池串拆成可去重的段？
**决定**：按 ASCII `.` 切分；空段保留（`"a..b"`→`["a","","b"]`）；无点串 = 单段
（`["int"]`）；空串 = 单空段（`[""]`）。重组即 `segs.join(".")`，对任意字节串**双向无损**
（切分是 `.` 处分割、重组是 `.` 处拼接，互逆）。即使串是浮点常量 `"3.14"` 或用户数据
`"a.b"` 也无损（只是段去重收益低，正确性不受影响）。
**为什么无损可靠**：不依赖「串一定是 FQ 名」的假设——切/拼在 `.` 上严格互逆。

### D2: 段字典 first-seen 序——确定性 = 自举不动点前提

**问题**：段字典的段顺序必须确定，否则 zbc 字节漂移、gen1≠gen2。
**决定**：遍历池串 `i = 0..strCount-1`（既有 `ZbcStringPool` 池序，已 1:1 稳定），对每
串按 `.` 切分，逐段 first-seen 分配递增 segIdx（复用 `ZbcStringPool` 的 intern 幂等）。
段序纯由池序派生 → 完全确定 → byte-identical 可复现。

### D3: 串索引不变——把改动锁在 STRS 段内

**问题**：全仓上百处 `pool.Idx(name)` 写 u32、reader `pool[idx]` 读——若改「名字=段序列」
会波及每个引用点，巨大。
**决定**：**strTable 顺序严格等于旧池顺序**，串索引 i 仍是「第 i 个池串」。segment-dict
只是 STRS 段把「第 i 个串的字节」换成「第 i 个串的段索引序列」。reader 在解析 STRS 时
就地重组出 `string[i]`（Rust 填 `Vec<String>`、z42 填 `string[]`），对上层完全透明。
→ SIGS/MODS/TSIG/NSPC/EXPT/DEPS/IMPT/TIDX 全部零改动，风险面收敛到 4 个 STRS 站点。

### D4: varint = 无符号 LEB128（protobuf 风格）

**问题**：段长、段数、段索引用定宽还是变长？
**决定**：无符号 LEB128（每字节低 7 位数据、高位续标志）。段长 ≤71B → 1 字节；段数
通常 1–4 → 1 字节；段索引 0–1005（z42.core）→ 1–2 字节。实测已按此计得 −44.4%。
writer 加 `ByteWriter.WriteVarint`，三 reader 各加 `read_varint`；与 stdlib
`z42.io.binary` 的 `WriteVarint`（`BinaryWriter.z42:197`）同算法，但二进制格式原语独立
（不引 stdlib 依赖进编译器 IR 层）。

### D5: 分阶段引入纪律——本次是纯格式 bump，不用新语法/API

本变更**不改 z42c 源码用的语法**、**不新增 z42c/xtask 源引用的 stdlib API**——只改
writer emit 的字节 + reader 解析。故 [bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md)
的「support 先行、use 晚一 nightly」语法/API 轴**不触发**。格式轴的坎（新 VM strict-pin
读不了旧种子）由已落地的**两代自举**（`fix-bootstrap-format-bump-deadlock`，8318ad7e
CI 快路径已绿）吸收。**本次将是两代自举 bump 路径的首次真实 CI 触发**——即其终极验证。

### D6: 与 indexed / sidecar 的关系

indexed zpkg（0.24）的散装 fullMode zbc 各自带 STRS → 自动跟随新编码（同一 `BuildStrs`）。
`.zsym` sidecar 的 symPool STRS 亦经 `BuildStrs` → 自动跟随。Change B 的 `.zsig` 尚未存在，
不受影响。

## Implementation Notes

- **段切分实现**：`BuildStrs` 内对 `pool.At(i)` 手动扫描 `.`（z42 `String.IndexOf`/
  `Substring` 或逐字符），产 `int[] segSeq`；段入 `ZbcStringPool segDict`。避免依赖尚不确定
  可用的 `String.Split`。
- **varint 读边界**：三 reader 的 `read_varint` 必须做溢出/越界保护（Rust `bail!`，z42
  游标 Pos 检查）。段索引越界（≥segCount）→ 报错，不静默。
- **empty.zbc 影响**：空模块也有池串（模块名 `""`? 见 `InternPoolStrings` 首行
  `pool.Intern(irm.Name)` + `"?"` 占位）→ STRS 非空 → golden hex 必变，按 version-bumping.md
  第 5 步从 regen 后 fixture 重截 247B（长度可能变）。
- **双 profile VM**：格式 bump 须 `cargo build`（debug）+ `--release` 都重建，否则 regen
  波用 debug VM（旧 minor）读新 fixture 全红（indexed 时踩过）。

## Testing Strategy

- **单元（z42c）**：`zbc_tests.z42` empty byte-identical（重截 hex）；新增一个「多段共享前缀」
  串池的往返单测（intern `Std.A`/`Std.B`/`Std.A.C` → BuildStrs → ZbcReader._readStrs →
  断言还原三串 + 段字典去重到 `Std`/`A`/`B`/`C`）。
- **Rust reader**：`loader_tests` / `zbc_compat` 读 committed fixture（segment-dict）还原串。
- **Golden / 端到端**：`xtask test`（regen 波 + e2e + cross-zpkg + stdlib + compiler +
  vscode-syntax）全绿；**自举不动点 gen1==gen2 byte-identical**（段序确定性验证）。
- **两代自举**：本地已用人造 0.25 bump 验过两代编排（design D7 of fix-bootstrap）；本次真实
  bump push 后盯 CI 两代 bump 路径。
