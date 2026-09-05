# Proposal: zpkg 内部内容标识改用 MurmurHash3 x86_128

## Why

z42c 是解释执行的，密码学哈希在编译热路径上贵得不成比例。800 KB 输入的实测：

| 算法 | 指令数 | 墙钟 |
|---|---|---|
| BLAKE3-128 | 6.21 G | 0.41 s |
| SHA-256 | 6.38 G | 0.40 s |
| 按 32 位字的快哈希 | **0.41 G** | **0.03 s** |

编译负载里 `Std.Crypto` 叶自占 **10.09%**（Blake3 4.75 + 1.98、Sha256 2.43）。而这些开销买到的
密码学强度**在这两处用途上一分钱都没花出去**：

- **BLID build_id**：只用来把 `main.zpkg` 与 `sidecar.zsym` 配成一对。Rust 侧只**读取两个值比相等**，
  从不重算 —— `metadata::build_id::compute` 在整个 runtime 里一次都没被调用（只在注释里出现）。
- **`source_hash`**：只做相等性比较判断某个 `.z42` 要不要重编。Rust 侧 `formats.rs` 只有这个字段，
  从不重算或校验。

两处都不参与信任决策、都没有跨语言互操作要求，因此都不需要抗碰撞的密码学哈希。

## What Changes

- 新增 `Z42.IR.Murmur3`（MurmurHash3 x86_128），及其参考向量对拍单测。
- `ZpkgWriter.WritePackedWithSidecar` 的 BLID build_id：`Blake3.HashLen(mainBytes, 16)` → `Murmur3.Hash128(mainBytes)`。
- 新增 `ZpkgBuilder.SourceHashHex`（`"mmh3:"` 前缀），源变更检测的三个生产调用点切过去。
  `ZpkgBuilder.Sha256Hex` **保留**，作为需要密码学强度时的入口。
- 文档同步：BLID / MODS `hash` 字段的算法与「为什么不用密码学哈希」。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.ir/src/Murmur3.z42` | NEW | MurmurHash3 x86_128 实现 |
| `src/libraries/z42.ir/tests/murmur3.z42` | NEW | 参考向量对拍（长度 0..33 全枚举 + 高位字节 + SourceHashHex 形态） |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | BLID build_id 换算法；去掉 `using Std.Crypto` |
| `src/libraries/z42.ir/src/ZpkgBuilder.z42` | MODIFY | 新增 `SourceHashHex`；`Sha256Hex` 注释改为「密码学强度入口」 |
| `src/libraries/z42.ir/z42.ir.z42.toml` | MODIFY | `z42.crypto` 依赖注释更新（依赖保留） |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `SrcReadHashTask.Run` 改调 `SourceHashHex` |
| `src/compiler/z42c.pipeline/src/Z42cCompiler.z42` | MODIFY | 同上 |
| `src/compiler/z42c.pipeline/src/Z42cReplCompiler.z42` | MODIFY | 同上 + using 注释 |
| `src/compiler/z42c.pipeline/tests/incremental/incremental_tests.z42` | MODIFY | 测试用的合成 hash 跟随生产路径 |
| `docs/book/src/compiler/zpkg-format.md` | MODIFY | BLID 与 MODS `hash` 的算法 + 理由（SoT） |
| `docs/book/src/dev/build.md` | MODIFY | 「忽略 BLID」一段的算法名 |
| `docs/design/runtime/zbc.md` | MODIFY | BLID 布局与写入时机 |
| `docs/design/runtime/zpkg.md` | MODIFY | BLID 字段说明 |
| `docs/design/runtime/vm-architecture.md` | MODIFY | 「BLID 算法」决策行 |
| `docs/design/language/exceptions.md` | MODIFY | strip 产物描述 |
| `docs/design/compiler/compilation.md` | MODIFY | `source_hash` 示例与增量语义描述 |
| `src/runtime/src/metadata/build_id.rs` | MODIFY | 删死函数 `compute`（BLAKE3 重算器，**零生产调用点**）+ 模块 doc 改写为「runtime 只比相等、算法只在写入端」 |
| `src/runtime/src/metadata/build_id_tests.rs` | MODIFY | 随之删掉 `compute` 的 4 条测试（`short_hex` 的保留） |
| `src/runtime/src/metadata/loader/artifact.rs` | MODIFY | 散装 `zbc_hash` 旁的注释：改为对照「它才是真跨语言契约」 |

**只读引用**：

- `src/runtime/src/loader/artifact.rs` — 确认散装 zbc `zbc_hash` 才是真跨语言契约
- `src/runtime/src/loader/artifact.rs`（散装 zbc 校验路径）—— 确认 `zbc_hash` 由 Rust 重算
- `scripts/build/xtask_compiler.z42` — 确认 `_ensureBootstrapSelfDepLibs` 已破 z42c⇄z42.ir 环

## Out of Scope

- **散装 zbc 内容哈希**（`IndexedDist.z42:33` 的 `zbc_hash`）——Rust `loader/artifact.rs:227` 用
  `blake3::hash` **重算校验**，是真的跨语言契约，本变更不动。
- `Std.Crypto` 的 `Sha256` / `Blake3` 实现本身不动 —— 它们仍服务真正需要密码学强度的调用方；
  本变更只是停止把它们用在非密码学用途上。
- `src/libraries/z42.ir/tests/{smoke,zpkg,depindex}/` 三个测试包当前不被任何 harness 发现
  （见 tasks.md 备注）——独立问题，独立变更。

## Open Questions

无（算法选型与失效语义见 design.md，已定）。
