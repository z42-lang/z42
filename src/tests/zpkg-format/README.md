# zpkg-format

## 职责

`.zpkg` v0 wire format 的字节级 golden fixture 集合。固化 ZpkgWriter 当前 emit 行为，防止 wire layout 在 minor bump 之间偷偷漂移。

每个 fixture 目录 = 一种代表性 zpkg layout：

| Fixture | 覆盖 |
|---------|------|
| `packed-minimal/`     | 单 class 单模块；packed mode 基础形态（META + STRS + NSPC + DEPS + SIGS + MODS + IMPL + BLID）|
| `packed-multi-module/`| 多 .z42 → 同一 zpkg；MODS 多条目 + 共享 STRS pool |
| `indexed-minimal/`    | indexed 模式（0.24 重定义，add-indexed-zpkg-min-patch）：主文件 = packed 段面去 MODS 加 FILE；配套散装 `source.zbc`（自包含 fullMode）供 VM indexed 装载测试（`loader_tests.rs::indexed_zpkg_*`）。冻结已解除，随 minor bump 正常 regen |
| `sym-only-sidecar/`   | `FlagSymOnly` set；只含 META + STRS + MDBG + BLID（sym-only sidecar 形态）|

> `packed-minimal` / `packed-multi-module` 只要 zpkg 内有 ≥1 个模块即触发 TSIG + IMPL section emit（`ZpkgWriterZ._buildSectionList`：`ExportedCount > 0` → secCount=9），故这两个 fixture 已天然覆盖 TSIG/IMPL 布局，不再需要单独的 `with-tsig` fixture。

## 核心文件
| 文件 | 职责 |
|------|------|
| `<fixture>/source.z42`（或 `mod_a.z42` + `mod_b.z42`） | z42 源（check in）|
| `<fixture>/<fixture>.z42.toml` | **构建配方**（check in；refresh-format-fixtures 2026-09-04 新增）—— `[project].pack` 决定 packed/indexed，是否带 `--release` 决定 strip/sidecar |
| `<fixture>/source.zpkg`     | z42c 输出字节基线（check in；regen 后 git diff = 实际格式变化）|

## 维护流程

正当 wire format 变化时（minor bump）：按各 fixture 自带的 `<fixture>.z42.toml` 重新 build、覆写 `source.zpkg`，`git diff` review 后连同 fixture 一起 commit。

```bash
# 前置：xtask build compiler && xtask build stdlib（fixture 须由新 writer emit）
export Z42_LIBS=$PWD/artifacts/build/libraries/dist/release
VM=./artifacts/build/runtime/release/z42vm
DRV=artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg
cd src/tests/zpkg-format
for d in packed-minimal packed-multi-module sym-only-sidecar; do
  (cd $d && $VM $DRV -- build $d.z42.toml --release)
done
(cd indexed-minimal && $VM $DRV -- build indexed-minimal.z42.toml)   # indexed 是 dev-mode，不加 --release
# 各自把 dist/<name>.zpkg 覆写为 source.zpkg；sym-only-sidecar 取 dist/demo.sidecar.zsym
```

> **历史**：配方此前只存在于口头（本 README 曾挂着「暂需手工用 `z42c build` 逐个重生」的 TODO），
> 于是 zbc 1.37→1.38 那次 bump 漏掉了本目录 —— `packed-multi-module` 停在 zpkg 42、
> `sym-only-sidecar` 停在 **35**（落后 8 个 minor）。把配方 check in 成 toml 就是为了让这一步可复现。

## 测试 harness

| 测试 | 检查内容 |
|------|---------|
| `tests/format_fixture_versions.rs` | **防腐门**：读 committed 字节，断言 header 版本 == 当前 `ZPKG_VERSION_*` / `ZBC_VERSION_*` 常量。fixture 陈旧 = 红测试 |
| `lazy_loader_tests.rs` | 把 `packed-minimal/source.zpkg` 当真实 zpkg 用于 colocated-dep 搜路径测试 |
| `loader/loader_tests.rs` | `indexed_zpkg_*` 真正加载 `indexed-minimal/` 的 indexed 主文件 + 散装 zbc |

> ⚠️ 覆盖不均：只有 `packed-minimal` 与 `indexed-minimal` 被功能测试消费；
> `packed-multi-module` / `sym-only-sidecar` 仅由上面的版本防腐门覆盖（此前**完全无人读**，
> 正因如此才会悄悄烂掉 8 个 minor）。

## 入口点

- 防腐门：`cargo test --test format_fixture_versions`
- 功能消费：`cargo test lazy_loader` / `cargo test indexed_zpkg`

## 依赖关系

- 上游：`z42c build`（自举编译器，`src/compiler/z42c.driver`）产出各 fixture 的 `source.zpkg`
- 下游：`FormatGoldenTests` harness + `FormatInvariantTests`
