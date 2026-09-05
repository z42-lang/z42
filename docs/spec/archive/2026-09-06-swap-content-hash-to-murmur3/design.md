# Design: zpkg 内部内容标识改用 MurmurHash3 x86_128

## Architecture

```
             ┌──────────────── z42.ir (stdlib) ────────────────┐
 z42c 源     │                                                  │
  ├─ driver/Main.z42 ──────► ZpkgBuilder.SourceHashHex ─┐        │
  ├─ pipeline/Z42cCompiler ─┘                            ├─► Murmur3.Hash128Hex
  ├─ pipeline/Z42cReplCompiler ───────────────────────────┘        │
  └─ (写包) ──────────────► ZpkgWriter.WritePackedWithSidecar      │
                                    └─ BLID ────────────► Murmur3.Hash128
             │                                                  │
             │  ZpkgBuilder.Sha256Hex（保留）──► Std.Crypto.Sha256│
             └──────────────────────────────────────────────────┘

Rust 侧：BLID 只比相等（metadata/build_id.rs::compute 无调用点）；
        source_hash 在 formats.rs 只当不透明字段存取。
        散装 zbc 的 zbc_hash 仍由 loader/artifact.rs 用 blake3 重算校验 —— 不动。
```

## Decisions

### Decision 1：放弃密码学强度

**问题**：BLID / source_hash 用的是密码学哈希，但两处都不是安全边界。

**选项**：
- A — 保留密码学哈希：安全直觉上"稳"，但为一个不存在的威胁模型付 10% 的编译时间。
- B — 换非密码学哈希：省掉这 10%，代价是失去抗碰撞（对抗性构造）的保证。

**决定**：选 B。判据是**这两个值是否参与信任决策**——都不参与：BLID 只做「这个 `.zsym` 配不配这个
`.zpkg`」的识别，source_hash 只做「这个文件变了没有」的相等性比较。能对抗性构造碰撞的攻击者早已
能直接改写 zpkg 本身，哈希强度不构成任何额外防线。真正需要强度的调用方继续用 `Std.Crypto`。

### Decision 2：算法选 MurmurHash3 **x86**_128，不是 x64_128

**问题**：MurmurHash3 的 128 位输出有 x86 与 x64 两个变体，值不同。

**决定**：选 **x86_128**。z42 **没有逻辑右移 `>>>`、也没有无符号整数类型**：

- x86 变体的 lane 是 32 位 ⇒ 可以用 `long` 承载 + `& 0xFFFFFFFF` 掩码，值恒为非负，
  `>>` 就等价于逻辑右移，全程无符号语义都是显式的、可读的。
- x64 变体的 lane 是 64 位 ⇒ 在没有无符号类型的语言里必须手工模拟 64 位无符号移位，
  极易出符号 bug，而且这类 bug 只在特定输入上暴露、很难靠少量向量测出来。

乘法按 wrapping 语义取低位（z42 的 `imul` 是 wrapping）。种子固定 0，输出 16 字节 = h1..h4
各按小端写出，与 smhasher 的 `MurmurHash3_x86_128` 逐字节一致。

### Decision 3：`Sha256Hex` 保留，另开 `SourceHashHex` 入口

**问题**：直接把 `Sha256Hex` 的实现换掉最省事，但名字会撒谎。

**决定**：新开 `SourceHashHex`，`Sha256Hex` 原样保留。一个叫 `Sha256Hex` 的函数返回的不是 SHA-256
是纯粹的陷阱；且 `zpkg_tests.z42` 有一条断言 SHA-256 标准向量的测试，保留它同时也保住了那条测试的
意义。改完后 `z42.crypto` 依赖仍需保留（`Sha256Hex` 还在用）。

### Decision 4：前缀 `"mmh3:"` 即失效开关

**问题**：换算法后，旧缓存 / 旧产物里的 `"sha256:…"` 与新算的值该如何互动？

**决定**：新值带 `"mmh3:"` 前缀。它与旧的 `"sha256:"` 天然不等 ⇒ 混版本时增量构建判定「全变了」→
一次全量重编 —— 正是想要的失效语义，**不需要任何版本号或迁移代码**。

### Decision 5：不 bump zpkg/zbc 格式版本

**问题**：产物字节变了，要不要 bump minor？

**决定**：不 bump。格式**布局**完全没变（BLID 仍是最后一段、仍 16 字节；MODS 的 `hash` 仍是一个池串）。
变的只是字段**内容**，而 runtime 对这两个字段都不做算法相关的解释：BLID 比相等、source_hash 不读。
strict-pin 的意义是「reader 能否正确解析 writer 的布局」，此处无关。

## Implementation Notes

- **自举种子**：`SourceHashHex` / `Murmur3` 是 z42.ir 的**新 API**，而 z42c 源立刻就用它。通常这会
  踩 [bootstrap-seed.md 轴②「用新 stdlib API 要晚一个 nightly」]，但 z42.ir 是**轴④**的特例——
  `_ensureBootstrapSelfDepLibs`（`scripts/build/xtask_compiler.z42:86`）在建 z42c **前**总是用当前源
  重建 z42.ir 进 build-libs（**不 warm-skip**，正是为了「z42c 用到 z42.ir 新 API」这个场景）。故无需等 nightly。
- **`_rotl` 的前提**：入参必须已掩码到 32 位（非负），否则 `x >> (32-r)` 会做算术右移灌进符号位。
  实现里每个 `_mul` / `_rotl` 都以 `& MASK32` 收尾来维持这个不变式。
- `Hash128Hex` 用字符串拼接而非 `StringBuilder`：z42.ir 不依赖 z42.text，为 32 个字符引入一整个包不值当；
  且它每包只调一次，不在热路径上。

## Testing Strategy

- **单元（算法）**：`src/libraries/z42.ir/tests/murmur3.z42` 对拍 smhasher 参考实现产出的向量——
  - 长度 **0..33 全枚举**（尾部 fallthrough 的 15 条分支、整块循环、块+尾混合各走一遍）；
  - **高位字节**（UTF-8 非 ASCII，2/6/18/30 字节）——抓 `& 0xFF` 掩码漏写导致的负数；
  - 摘要形态（32 个小写 hex）+ 雪崩（1 比特翻转 → 完全不同）。
- **单元（形态）**：`SourceHashHex` 返回 `"mmh3:" + 32 hex`。
- **自举不动点**：`xtask test compiler`（gen1 == gen2；BLID 本就 ignored）。
- **完整 GREEN**：`xtask test` 全 stage。
