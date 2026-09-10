# Proposal: 给分代模式补 CI 覆盖 —— 顺带修出三处漏掉的数组写屏障

## Why

`Z42_GC_MODE=generational` **从来没有被 CI 跑过**（`grep Z42_GC_MODE` 全仓只有一个
concurrent 模式的 smoke）。这是 **#537 / #539 三个「丢对象」缺陷能活几个月**的直接原因，
也是翻 `Z42_GC_MODE` 默认之前必须先还的债 —— 分代现在中位停顿比 STW 低 62%，
值得打开，但没有覆盖就不能打开。

## What Changes

- **新 gate stage `gc generational (z42c.semantics build)`**：用分代收集器 + 16 MB nursery
  重编一个真实包。7.1 s，可 `--skip gcgen`
- **修三处漏掉的数组写屏障**（找 bug 时挖出来的，与上面的 stage 无关但同源）：
  `Array.SetValue` / `Array.Copy`（`__array_copy`）/ 经 `ref` 写数组元素 ——
  三处都往数组里写堆引用却**不发写屏障**，而解释器的 `ArraySet` 与 JIT 的数组存储 helper
  一直都发
- 三个回归测试（都验证过「不打补丁就红」）

## ⚠️ 本 change **没有**修好的：一个 known-open 缺陷

追 stage 该用多大 nursery 时，撞上一个**在 main 上就存在**的缺陷：

```bash
env Z42_GC_MODE=generational Z42_GC_NURSERY_BYTES=1M \
  artifacts/build/runtime/release/z42vm artifacts/.z42/programs/z42c/z42c.driver.zpkg \
  -- build src/compiler/z42c.semantics/z42c.semantics.z42.toml --release --no-incremental
# → 96 次 minor 之后：__str_hash_code: arg 0 expected string, got Null
```

**3/3 必现，且早于 #552 / #553**（拿 `git checkout 3935d8f1 -- src/runtime` 对照过）。
2M 及以上全绿。探针形状：**老数组（age 2）、卡是脏的、chunk 未被 TLAB 借出，
孩子却被扫掉了** —— 不是漏发屏障，是标记阶段没走到。详见 design.md「未修完的那一个」。

**stage 的 nursery 因此定在 16M**（能抓到 #539 那一类、且在干净树上是绿的），
不是 1M。这一点在代码注释、book 与本文档三处都写明了。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/test/xtask_test.z42` | MODIFY | 新 stage + `_testGenerationalWorkload()` |
| `docs/book/src/dev/test-gate.md` | MODIFY | `gate-stages` 区 + 流水图（与代码互为副本，有门对账） |
| `src/runtime/src/corelib/array.rs` | MODIFY | `Array.SetValue` / `Array.Copy` 补屏障 + `barrier_copied_range` |
| `src/runtime/src/interp/frame.rs` | MODIFY | `RefKind::Array` 写回补屏障 |
| `src/runtime/src/gc/heap.rs` / `arc_heap/interface.rs` | MODIFY | `array_card_dirty_for_test`（测试用） |
| `src/runtime/src/corelib/array_tests.rs` | MODIFY | 三个屏障回归测试 |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | CI 覆盖 + known-open 缺陷的复现方 |
| `docs/spec/changes/add-ci-generational-coverage/` | NEW | 本变更容器 |

## Out of Scope

- 🔴 **修上面那个 known-open 缺陷** → 单独立项，已留下精确复现与探针形状
- **翻 `Z42_GC_MODE` 默认** → 前置是上面那个缺陷修完（否则小 nursery 下不可用）

## Open Questions

- [ ] 无。
