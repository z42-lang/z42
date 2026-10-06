# zbc-format

## 职责
`.zbc` wire format 的字节级 golden fixture 集合。固化 `ZbcWriter` 当前 emit 行为，防止 wire layout 在 minor bump 之间偷偷漂移。

每个 fixture 目录 = 一种代表性 zbc layout：

| Fixture | 覆盖 |
|---------|------|
| `empty/`              | 最小有效 module（`void Main() { }`），无类、无 stdlib 调用 |
| `strp-func-minimal/`  | 单类单方法 — STRP + TYPE + FUNC + DBUG 基础组合 |
| `multi-method/`       | 多方法 + 同类内 cross-method 调用 — 更密集的 line table |
| `with-tidx/`          | `[Test]` 注解触发 TIDX 段 |
| `cross-import-token/` | `Std.IO.Console` 调用触发 IMPT 段 + IMPORT_BASE 0x8000_0000 token |
| `with-frcs/`          | 方法组转换（发 `LoadFn`；目录名沿用历史，writer 不发射 FRCS 段） |

BLID section 只在 stripped mode 出现（`--emit zbc` 默认不 strip），fixture 集合不覆盖。

## 如何测试验证
```bash
cargo test --test zbc_compat    # Rust 解码这些 source.zbc 字节基线，验证 reader 兼容当前 wire format
```
harness：[`src/runtime/tests/zbc_compat.rs`](../../../../../runtime/tests/zbc_compat.rs)。

## 维护流程
正当 wire format 变化时（minor bump）：

```bash
./xtask build compiler && ./xtask build stdlib   # fixture 须由新 writer emit；cross-import-token / with-tidx 需 stdlib 解析
./xtask build test                               # 就地重生全部 golden，含本目录各 source.zbc
git diff src/compiler/z42.package/tests/fixtures/zbc-format/   # review 哪些 fixture 受影响
```

`build test` 对本目录特判：直接覆写各 fixture 的 `source.zbc`（其余 run-golden 落 artifacts 镜像）。fixture 须随 bump 一同提交。
流程见 [version-bumping.md](../../../../../../docs/agent/rules/version-bumping.md) 步骤 4。

`zpkg-format/` 的 4 份 fixture `build test` **不碰**，按 [zpkg-format/README.md](../zpkg-format/README.md) 从各自的 `<name>.z42.toml` 逐个重建（步骤 9）；其消费方是 Rust 单测，故 `build test` 后 `git status` 干净**不能**证明它们是最新的。

## 核心文件
| 文件 | 职责 |
|------|------|
| `<fixture>/source.z42` | z42 源 |
| `<fixture>/source.zbc`  | z42c 输出字节基线（regen 后 git diff = 实际格式变化）|

不放解码后形态的 `expected.json` 快照：无代码读取、无防腐门的快照会腐坏并误导排查。
