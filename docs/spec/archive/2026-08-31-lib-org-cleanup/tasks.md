# Tasks: stdlib 组织清理（library-review PR-C）

> 状态：🟢 已完成 | 创建：2026-08-31 | 完成：2026-08-31 | 类型：refactor(stdlib)（最小化流程）

## 背景

基于 `docs/library_review.md` 的 stdlib 迭代第一波 PR-C：两项纯组织清理，不改任何外部 API 行为。

- **O1**：删 `z42.regex` 对 `z42.collections` 的**死依赖**——regex 源零引用 `Stack`/`Queue`/`Collections`
  （`grep` 确认），manifest 里那行 dep 是历史遗留。
- **O2**：把 3 文件碎包 `z42.io.binary`（`BinaryReader`/`BinaryWriter`/`BinaryException`）**并入 `z42.io`**。
  命名空间不变（`Std.IO.Binary` + `Std`），下游 `using Std.IO.Binary;` 照旧可用。`z42.io.binary` 的
  deps（core/encoding/io）是 `z42.io`（core/encoding/text）的子集 + 自身，无需给 `z42.io` 加新 dep。

## 进度

- [x] O1: 删 `z42.regex.z42.toml` 的 `z42.collections` dep 行
- [x] O2.1: `git mv` `Binary{Reader,Writer,Exception}.z42` → `z42.io/src/`
- [x] O2.2: `git mv` 6 个 binary 测试 → `z42.io/tests/`
- [x] O2.3: 删 `z42.io.binary/`（toml + README + 空 src/tests 目录）
- [x] O2.4: `z42.workspace.toml` default-members 去掉 `z42.io.binary`
- [x] 文档同步：libraries/README、design/stdlib（organization/overview/roadmap/io-binary/regex）
- [x] GREEN：完整 `xtask test` → ✅ all stages passed；self-host 3/3 gen1==gen2；z42.io 51/51
- [x] 归档 + PR

## GREEN 备注

- 目标 `xtask test stdlib z42.io` 51/51 通过（含并入的 6 个 binary 测试）；完整 `xtask test` 全绿。
- **一条 benign WARN**：`cannot read transitive dep zpkg meta z42.io.binary.zpkg`——查明为 nightly
  **种子**仍携带旧 `z42.io.binary.zpkg`（`.z42/libs/` + 测试 harness 复制进 scratch 的字节相同副本）所致；
  `artifacts/build/libraries/` 无 io.binary，无任何**新建** zpkg 记录它为 dep，无源 toml 引用它。属种子过渡
  噪声，合并后新 nightly 一出即消失，不 fail 任何 stage（exit 0）。

## 验证要点

- 无下游 toml 依赖 `z42.io.binary`（grep 确认仅其自身 + workspace）。
- 无 CI / test-script 按包名 `z42.io.binary` 枚举（grep `scripts/` `.github/` 确认）。
- runtime Rust 对 `Std.IO.Binary` 的引用均为注释 + 通用 namespace 派生逻辑（与 zpkg 名无关），无需改。

## 备注

- 无 zbc/zpkg 格式 bump。
- io-binary.md 保留为 `Std.IO.Binary` 机制文档，仅更新包路径。
