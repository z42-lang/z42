# Proposal: indexed zpkg 重设计——最小 patch 分发

> 前置 `add-file-level-incremental`（✅ 2026-07-08 归档）。ir 类变更（zpkg 格式）→ 完整流程。
> 占用 `compiler` + `runtime` 双锁（ACTIVE.md 已登记）。

## Why

1. **User 需求（2026-07-05/07 裁定）**：用户更新 **patch 最小**——indexed 布局下改一个源文件,
   dist 里**只有主 zpkg（索引，允许变）+ 对应文件的 `.zbc`** 变化，其余未改动文件的散装
   zbc **逐字节不动**（含 mtime）→ 增量分发只传主文件 + 变更 zbc 子集。
2. **旧（C# 时代）indexed 设计做不到**：散装 zbc 是 stripped 形态（仅 BSTR/FUNC），签名引用
   主文件**全局 SIGS/STRS 池**——改一个文件扰动全局池，其它 zbc 的池索引可能连带漂移。
   z42c 自举重写从未实现该模式，VM reader 显式 bail（`self-hosting-future-indexed-zpkg`）。
3. **前置已备好物质基础**：add-file-level-incremental 的 cache 条目 = **自包含 fullMode zbc**
   （每文件独立池，源不变 → 字节不变，对账器 47/47 实证）——indexed 的散装 zbc 就是它的
   原样投影，dist 侧零重序列化。
4. **`pack` 字段闭环**：manifest 已解析 `pack`（三层优先级、内置默认 debug→indexed /
   release→packed 均已写入 project.md），但 z42c 从未消费——本 change 让它生效。

## What Changes

1. **indexed 布局重设计（zpkg minor bump，zpkg-only 路径）**：
   - **主 zpkg**：段面与 packed 相同（META/STRS/NSPC/EXPT/DEPS/SIGS/TSIG/IMPL——跨包消费方
     DepScan/TSIG 读取零改动），**MODS 替换为 FILE 段**（每文件:rel 路径 / 源 SHA-256 /
     ns / zbc 文件内容 hash）。主文件每次构建可整体重写（User 已确认）。
   - **散装 zbc**：`<dist>/<rel 去 .z42>.zbc`，**自包含 fullMode**（= cache 条目字节原样
     复制，含 DBUG）；未失效文件**不重写不触碰**。
2. **`pack` 生效**：三层优先级（profile > exe > project）+ 内置默认（debug→false=indexed，
   release→true=packed），`z42c build` 按其选 packed/indexed 写出；`pack=false ∧ strip=true`
   → 诊断错误（indexed 为开发态，DBUG 内嵌散装 zbc；zsym 归 packed）。
3. **VM indexed 装载**（zbc_reader.rs）：flags=indexed → 读 FILE 目录 → 逐 zbc 相对主文件
   目录加载（复用 standalone zbc 解码路径）→ ns/entry/DEPS/SIGS 来自主文件；zbc 内容 hash
   校验不符 → 明确报错（部署一致性）。strict-pin 常量同 commit bump。
4. **增量投影**：indexed dist 写出 = 失效文件 zbc 复制（自 cache）+ 主文件重写；
   `--no-incremental` 全部重写。对账器扩展 indexed 腿：增量 dist == 全量 dist（逐文件字节）
   **且**未 touch 的 zbc 文件未被重写（mtime 断言）。
5. **fixture/文档**：`src/tests/zpkg-format/indexed-minimal/` 用 z42c 按新布局重生（解除
   minor=22 搁浅）；zpkg.md FILE 段/模式表/changelog 改写；version-bumping 步骤 6-9 全套。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | `WriteIndexedMain`（FILE 段 + flags）；`ZpkgWriterZ.Minor++` |
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | `Open` 放行 indexed 主文件（DepScan 读跨包 indexed lib 的 NSPC/SIGS/TSIG）|
| `src/compiler/z42c.project/src/PackageTypes.z42` | MODIFY | ZpkgFileZ 增 mode/FILE 条目模型（如需）|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | pack 三层决议 + indexed dist 投影（cache→dist 复制 + 主文件写出）+ strip 冲突诊断 |
| `src/compiler/z42c.driver/src/IncrementalDriver.z42` | MODIFY | 投影辅助（失效集 → 需复制的 zbc 清单）|
| `src/compiler/z42c.project/tests/`（新单元目录）| NEW | indexed 主文件写读单测 |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | indexed 装载 + `ZPKG_VERSION_MINOR` bump + changelog 行 |
| `src/runtime/src/metadata/`（相关 loader 拆分文件如涉及）| MODIFY | 装载路径接线 |
| `src/tests/zpkg-format/indexed-minimal/` | MODIFY | 新布局字节基线重生 |
| `src/tests/`（新 indexed run e2e 目录）| NEW | indexed exe 直跑 golden |
| `scripts/test/xtask_test_incremental.z42` | MODIFY | indexed 对账腿（字节 + mtime 断言）|
| `docs/design/runtime/zpkg.md` | MODIFY | FILE 段新布局 / 模式表 / Minor changelog |
| `docs/design/compiler/project.md` | MODIFY | pack 生效说明 + 产物输出表刷新 |
| `docs/design/compiler/self-hosting.md` | MODIFY | `self-hosting-future-indexed-zpkg` Deferred 关闭 |
| `docs/roadmap.md` | MODIFY | Deferred 索引行更新 |
| 相关 README（z42c.project / z42c.driver / runtime metadata）| MODIFY | 六段同步 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 双锁登记/释放 |

**只读引用**：`src/compiler/z42c.project/src/CacheStore.z42`（cache 条目形态 SoT）；
`src/runtime/src/metadata/zbc_reader.rs` 现 packed/zbc 解码路径；`.claude/rules/version-bumping.md`。

## Out of Scope

- 分发工具链（差量打包/签名/传输）——另行评估。
- 跨包上游变化 → 下游失效追踪（维持现状）。
- workspace 增量布线（`incremental-future-workspace-wiring` 另立）。
- zbc 格式：**不动**（散装 zbc = 既有 fullMode；仅 zpkg minor bump）。

## Open Questions（已在 design 裁定，见 D1-D8）

- [x] 聚合 TSIG 位置 → 主文件（与 packed 同段面，跨包消费零改动）——design D1
- [x] pack 组合矩阵 → project.md 既有表生效；`pack=false ∧ strip=true` 报错——design D4
- [x] `.zsym` 形态 → indexed 不产 zsym（DBUG 内嵌散装 zbc）——design D4
