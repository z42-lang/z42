# Tasks: zpkg 内部内容标识改用 MurmurHash3 x86_128

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

## 进度概览
- [x] 阶段 1: 算法实现 + 对拍
- [x] 阶段 2: 接线（BLID + 源哈希）
- [x] 阶段 3: 文档同步
- [x] 阶段 4: 验证

## 阶段 1: 算法实现 + 对拍
- [x] 1.1 `src/libraries/z42.ir/src/Murmur3.z42`：`Hash128` / `Hash128Hex`（x86_128，seed=0，小端 h1..h4）
- [x] 1.2 `src/libraries/z42.ir/tests/murmur3.z42`：对拍 smhasher 参考实现
      —— 长度 0..33 全枚举（尾部 15 条 fallthrough + 整块 + 块尾混合）、
      高位字节（UTF-8 2/6/18/30 字节）、摘要形态 + 雪崩

## 阶段 2: 接线
- [x] 2.1 `ZpkgWriter.z42`：BLID `Blake3.HashLen(mainBytes,16)` → `Murmur3.Hash128(mainBytes)`；删 `using Std.Crypto`
- [x] 2.2 `ZpkgBuilder.z42`：新增 `SourceHashHex`（`"mmh3:"` 前缀）；`Sha256Hex` 保留 + 注释改为密码学入口
- [x] 2.3 三个生产调用点切换：`z42c.driver/src/Main.z42:43`、
      `z42c.pipeline/src/Z42cCompiler.z42:37`、`z42c.pipeline/src/Z42cReplCompiler.z42:88`
- [x] 2.4 `z42.ir.z42.toml`：`z42.crypto` 依赖注释更新（依赖**保留**——`Sha256Hex` 仍用）
- [x] 2.5 `incremental_tests.z42`：合成 hash 跟随生产路径改用 `SourceHashHex`

## 阶段 3: 文档同步
- [x] 3.1 `docs/book/src/compiler/zpkg-format.md`：BLID 节 + MODS `hash` 字段（算法 + 「为什么不用密码学哈希」+ 「散装 zbc_hash 仍是 BLAKE3」的对照）
- [x] 3.2 `docs/book/src/dev/build.md`：「忽略 BLID」一段
- [x] 3.3 `docs/design/runtime/{zbc,zpkg,vm-architecture}.md`、`docs/design/language/exceptions.md`
- [x] 3.4 `docs/design/compiler/compilation.md`：`source_hash` 示例 + 增量语义描述

## 阶段 3.5: Scope 扩展（实施中发现，已记录）
- [x] 3.5.1 `src/runtime/src/metadata/build_id.rs`：删死函数 `compute`。
      它按 BLAKE3 重算 BLID，**生产调用点为零**（全 runtime 只有它自己的单测调它；
      装载路径走 `read_build_id` 读段 + `!=` 比相等）。换算法后它会算出一个与写入端
      不一致的值 —— 留着就是给未来的调用方埋雷。模块 doc 同步改写成
      「runtime 只比相等、算法只活在写入端，故此处刻意不放 compute」。
- [x] 3.5.2 `build_id_tests.rs`：删掉 `compute` 的 4 条测试（`short_hex` 的保留）。
- [x] 3.5.3 `loader/artifact.rs`：散装 `zbc_hash` 旁注释改为「它才是真跨语言契约」的对照。

## 阶段 4: 验证
- [x] 4.1 `xtask test stdlib z42.ir` —— 4/4 通过（41 条向量 + SourceHashHex 形态）
- [x] 4.2 `xtask test compiler` —— `✅ z42c self-host 不动点: 3/3 packages gen1==gen2`
      + `✅ z42c [Test]: all 23 unit(s) passed`
- [x] 4.3 完整 `xtask test` —— `✅ GREEN — all stages passed`（11 stage）
- [x] 4.4 性能对账（标准负载 `build z42c.semantics --release`，3 次，`/usr/bin/time -l`）：

      | | 指令数 | 墙钟 | 峰值 RSS |
      |---|---|---|---|
      | 基线（main 33ffb3ca，记录值） | 73.88 G | 6.07 s | 1.022 GB |
      | 本变更 | **66.24 G (−10.3%)** | **5.50 s (−9.4%)** | **0.987 GB (−3.4%)** |

      三次指令数 66.2224 / 66.2437 / 66.2447 G，离散度 0.034%（门限 <0.1%）。
      采样 profile 佐证：`Std.Crypto` 叶**从 10.09% 归零**（Blake3 / Sha256 一次都不出现），
      `Murmur3` 顶上 2.12%（`Hash128` 1.80 + `_rd32` 0.33）⇒ 净 −8 个百分点。

### 验收门 1 的处置（产物字节）
标准验收门 1 是「产物逐字节不变」，**本变更不适用**：改的就是写进产物的两个字段，
且 `"sha256:"`(71 字符) → `"mmh3:"`(37 字符) 长度不同 ⇒ 串池偏移整体平移，逐字节比对无意义。
替代证据：① 自举不动点 `gen1==gen2`（同一算法自我复现）；
② **`xtask test incremental`**（不在默认 gate 里，本次单独跑）：新哈希下
`demo 5/5 + xtask 62/62 files byte-identical`（whole dist）—— 逐文件 touch 后的
增量产物与全量产物**逐字节相同**，这正是源哈希变更检测的对口验证；
③ 297 条 e2e golden 全绿（产物语义未变）。

## 备注

**发现（Out of Scope，独立变更）**：`src/libraries/z42.ir/tests/{smoke,zpkg,depindex}/`
三个测试包**当前不被任何 harness 发现**，即从未运行。原因：它们是
`converge-z42c-ir-metadata` 从 `src/compiler/z42c.ir/tests/` 下沉过来的，沿用了
**编译器**侧的布局（`tests/<unit>/{<name>.z42.toml + *_tests.z42}`），而
`_testCompilerUnits`（`scripts/build/xtask_compiler.z42:261`）只扫 `src/compiler/<member>/tests`；
stdlib 侧的 `_discoverTestUnits`（`scripts/test/xtask_test_lib_units.z42:41`）只认
`tests/*.z42`（单文件）或 `tests/<name>/source.z42`（目录模式），两者都不匹配。
实测 `xtask test stdlib z42.ir` 在本变更加入 `tests/murmur3.z42` 之前报
「0 file(s) in 0 lib(s)」，加入后报「1 test unit(s)」——三个目录一个都没被拾起。
本变更因此把新测试写成单文件 `tests/murmur3.z42`（能跑的形态）。复活那三个包属独立问题。
