# zpkg-format

## 职责
`.zpkg` wire format 的字节级 golden fixture 集合。固化 `ZpkgWriter` 当前 emit 行为，防止 wire layout 在 minor bump 之间偷偷漂移。

每个 fixture 目录 = 一种代表性 zpkg layout：

| Fixture | 覆盖 |
|---------|------|
| `packed-minimal/`     | 单 class 单模块；packed mode 基础形态（META + STRS + NSPC + DEPS + SIGS + MODS + IMPL + BLID）|
| `packed-multi-module/`| 多 .z42 → 同一 zpkg；MODS 多条目 + 共享 STRS pool |
| `indexed-minimal/`    | indexed 模式：主文件 = packed 段面去 MODS 加 FILE；配套散装 `source.zbc`（自包含 fullMode）供 VM indexed 装载测试 |
| `sym-only-sidecar/`   | `FlagSymOnly` set；只含 META + STRS + MDBG + BLID（sym-only sidecar 形态）|

`packed-*` 只要 zpkg 内有 ≥1 个模块即触发 TSIG + IMPL section emit（`ZpkgWriterZ._buildSectionList`：`ExportedCount > 0`），故已覆盖 TSIG/IMPL 布局。

## 如何测试验证
```bash
cargo test --test format_fixture_versions   # 防腐门：committed 字节的 header 版本 == 当前 ZPKG_VERSION_* / ZBC_VERSION_* 常量；fixture 陈旧 = 红
cargo test lazy_loader                      # packed-minimal 作 colocated-dep 搜路径的真实 zpkg
cargo test indexed_zpkg                     # 加载 indexed-minimal 的 indexed 主文件 + 散装 zbc
```

消费方：`src/runtime/tests/format_fixture_versions.rs`、`src/runtime/src/metadata/lazy_loader_tests.rs`、`src/runtime/src/metadata/loader/loader_tests.rs`。
只有 `packed-minimal` 与 `indexed-minimal` 被功能测试消费；`packed-multi-module` / `sym-only-sidecar` 仅由版本防腐门覆盖。

## 维护流程
正当 wire format 变化时（minor bump）：按各 fixture 自带的 `<fixture>.z42.toml` 重新 build、覆写 `source.zpkg`，`git diff` review 后连同 fixture 一起提交。配方入库是为了让重生步骤可复现。

```bash
# 前置：./xtask build compiler && ./xtask build stdlib（fixture 须由新 writer emit）
export Z42_LIBS=$PWD/artifacts/build/libraries/dist/release
VM=./artifacts/build/runtime/release/z42vm
DRV=artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg
cd src/compiler/z42.package/tests/fixtures/zpkg-format
for d in packed-minimal packed-multi-module sym-only-sidecar; do
  (cd $d && $VM $DRV -- build $d.z42.toml --release)
done
(cd indexed-minimal && $VM $DRV -- build indexed-minimal.z42.toml)   # indexed 是 dev-mode，不加 --release
# 各自把 dist/<name>.zpkg 覆写为 source.zpkg；sym-only-sidecar 取 dist/demo.sidecar.zsym
```

流程见 [version-bumping.md](../../../../../../docs/agent/rules/version-bumping.md) 步骤 9。

## 核心文件
| 文件 | 职责 |
|------|------|
| `<fixture>/source.z42`（或 `mod_a.z42` + `mod_b.z42`） | z42 源 |
| `<fixture>/<fixture>.z42.toml` | **构建配方** —— `[project].pack` 决定 packed/indexed，是否带 `--release` 决定 strip/sidecar |
| `<fixture>/source.zpkg` | z42c 输出字节基线（regen 后 git diff = 实际格式变化）|
