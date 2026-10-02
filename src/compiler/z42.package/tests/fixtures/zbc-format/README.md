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

> 不放解码后形态的 `expected.json` 快照：无代码读取、无防腐门的快照会腐坏并误导排查。
> 若需要「格式 bump 时可读的 diff」，请**连同防腐门一起**引入（形态参考 `format_fixture_versions` 那道门）。

## 维护流程

正当 wire format 变化时（minor bump）：

```bash
xtask build compiler && xtask build stdlib   # 必要 — fixture 须由**新 writer** emit
xtask build test                             # 重生全部 golden（含本目录的 source.zbc，in-place）
git diff src/compiler/z42.package/tests/fixtures/zbc-format/               # review 哪些 fixture 受影响
```

`build test` 对 `zbc-format` 目录特判：直接覆写各 fixture 的 `source.zbc`（其余 run-golden 落 artifacts 镜像）。每次 bump 必须把 fixture 一同 commit。

> 注意：重生命令是 `xtask build test`（`xtask` 没有 `regen` 子命令），与
> [version-bumping.md](../../../../../../docs/agent/rules/version-bumping.md) 步骤 4 和 CI 的
> `compile-test-assets` job 用的是同一条（`test-host` 的 `test all` 走的也是同一个 regen）。
>
> ⚠️ 本目录**不覆盖** `src/compiler/z42.package/tests/fixtures/zpkg-format/`：那 4 份 fixture `build test` **不碰**，
> 得按 [zpkg-format/README.md](../zpkg-format/README.md) 从各自的 `<name>.z42.toml` 逐个重建
> （version-bumping.md 步骤 9）。它们的消费方是 **Rust 单测**，所以跑完 `build test` 后
> `git status` 干净**不能**证明它们是最新的。

## 测试 harness

| 测试 | 检查内容 |
|------|---------|
| `zbc_compat.rs` | Rust 解码这些 check-in 的 `source.zbc` 字节基线，验证 reader 兼容当前 wire format |

详见 [`src/runtime/tests/zbc_compat.rs`](../../../../../../src/runtime/tests/zbc_compat.rs)。

## 入口点

- 维护命令：`xtask build test`（不是 `regen` —— 那个子命令不存在，见「维护流程」的更正）
- 测试 harness：`cargo test --test zbc_compat`

## 依赖关系

- 上游：`z42 xtask.zpkg build stdlib` 产出（`artifacts/build/libraries/dist/release/*.zpkg`）—— `cross-import-token` / `with-tidx` 需要 stdlib 才能解析
- 下游：`zbc_compat.rs`（`src/runtime/tests/`）
