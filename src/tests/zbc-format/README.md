# zbc-format

## 职责

`.zbc` v1 wire format 的字节级 golden fixture 集合。固化 ZbcWriter 当前 emit 行为，防止 wire layout 在 minor bump 之间偷偷漂移。

每个 fixture 目录 = 一种代表性 zbc layout：

| Fixture | 覆盖 |
|---------|------|
| `empty/`              | 最小有效 module（`void Main() { }`），无类、无 stdlib 调用 |
| `strp-func-minimal/`  | 单类单方法 — STRP + TYPE + FUNC + DBUG 基础组合 |
| `multi-method/`       | 多方法 + 同类内 cross-method 调用 — 更密集的 line table |
| `with-tidx/`          | `[Test]` 注解触发 TIDX 段 |
| `cross-import-token/` | `Std.IO.Console` 调用触发 IMPT 段 + IMPORT_BASE 0x8000_0000 token |
| `with-frcs/`          | Method-group conversion 触发 FRCS（FuncRef cache slot）段 |

> BLID section 当前只在 stripped mode 出现（`--emit zbc` 默认不 strip），fixture 集合不覆盖；如果未来引入 stripped golden 测试，加 `stripped-with-blid/` 即可。

## 核心文件
| 文件 | 职责 |
|------|------|
| `<fixture>/source.z42`     | z42 源（check in）|
| `<fixture>/source.zbc`     | z42c 输出字节（check in；regen 后 git diff = 实际格式变化）|

> 每个 fixture 曾另有一份 `expected.json`（解码后形态的人类可读快照）。**2026-09-11 已删除**：
> 全仓无任何代码读它、也没有防腐门，于是一路腐坏到落后 **18 个 minor**（停在 `minor: 20`，
> 而彼时格式已是 38），还被当成可信事实误导过一次排查。
> 这是补完 [PR #424](https://github.com/z42-lang/z42/pull/424)（commit `aab7013b`）的同一次清理——那个 PR 已按同样理由删掉了
> `zpkg-format/*/expected.json` 全部 4 份，只是漏了本目录。
> 若将来需要「格式 bump 时可读的 diff」，请**连同防腐门一起**作为独立变更引入
> （形态参考 `format_fixture_versions` 那道门），不要再引入无门禁的快照文件。

## 维护流程

正当 wire format 变化时（minor bump）：

```bash
z42 xtask.zpkg build stdlib         # 必要 — fixture 用 stdlib 解析
z42 xtask.zpkg regen                # 用 z42c 重生全部 golden（含本目录的 source.zbc，in-place）
git diff src/tests/zbc-format/      # review 哪些 fixture 受影响
```

`xtask regen` 对 `zbc-format` 目录特判：直接覆写各 fixture 的 `source.zbc`（其余 run-golden 落 artifacts 镜像）。每次 bump 必须把 fixture 一同 commit。

## 测试 harness

| 测试 | 检查内容 |
|------|---------|
| `zbc_compat.rs` | Rust 解码这些 check-in 的 `source.zbc` 字节基线，验证 reader 兼容当前 wire format |

详见 [`src/runtime/tests/zbc_compat.rs`](../../../src/runtime/tests/zbc_compat.rs)。

## 入口点

- 维护命令：`z42 xtask.zpkg regen`
- 测试 harness：`cargo test --test zbc_compat`

## 依赖关系

- 上游：`z42 xtask.zpkg build stdlib` 产出（`artifacts/build/libraries/dist/release/*.zpkg`）—— `cross-import-token` / `with-tidx` 需要 stdlib 才能解析
- 下游：`zbc_compat.rs`（`src/runtime/tests/`）
