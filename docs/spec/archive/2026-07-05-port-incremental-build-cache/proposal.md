# Proposal: z42c 增量编译 cache 移植（单文件 .zbc 落盘 + 增量 probe）

## Why

1. **能力随 C# 删除而消失**：单文件 `.zbc` 编译缓存 + 增量编译是 C# 编译器
   `2026-04-27-incremental-build-cache`（C5）落地的能力（[project.md L3](../../../design/compiler/project.md)
   增量编译节）。自举 z42c 的 `build` 是 MVP「单工程/packed/无增量」
   （[Main.z42:84](../../../../src/compiler/z42c.driver/src/Main.z42#L84)），cache 落盘被显式延后
   （[ZpkgBuilder.z42:2](../../../../src/compiler/z42c.project/src/ZpkgBuilder.z42#L2)；roadmap Deferred
   `self-hosting-future-indexed-zpkg` 部分覆盖）。2026-06-26 删 C# 后**任何构建路径都不再产 cache**
   ——User 2026-07-05 实测 `z42 publish` 后 cache 目录缺失，裁决恢复（"zbc 是单文件编译后产物，
   有了这个可以增量编译"）。
2. **全量重编代价日增**：z42c 自建、stdlib 22 包、xtask 每次全量；per-file cache + hash probe
   是后续所有构建提速（含 REPL `repl-future-incremental-compilation`）的物质基础。

## What Changes

1. **cache 落盘（阶段 A）**：`z42c build` 逐文件产出 fullMode `.zbc` 到 cache 目录
   （复用既有 byte-identical `ZbcWriter.Write`——`--emit-zbc` 已在用）。cache 目录解析镜像
   `_resolveDistDir` 级联：`[build].cache_dir`（含 `${output_dir}` 模板）→ `${output_dir}/.cache`
   → 无 [build] 时 `<projectDir>/.cache`；workspace member 继承 `[workspace.build].cache_dir`。
2. **增量 probe（阶段 B；User 裁决 2026-07-05 采 C# 最终形态 `ed901f01`）**：对每个源文件校验
   ① SHA-256 == 上次 zpkg 记录 ② cache/<rel>.zbc 存在 ③ 上次 zpkg TSIG 含该模块 ns。
   **any fresh → 整包失效**（全量重编；混合 cached/fresh 重建因正确性问题被 C# 上游放弃，
   ZbcReader 消费路径实际不可达 → **不移植 ZbcReader**）；**100% 命中 → 完全跳过重写**
   （保留现有 zpkg 不动；exe 仍执行依赖复制保 dist 兄弟包新鲜）。
   `--no-incremental` 强制全量；`Z42_INCR_DEBUG=1` 输出逐文件 miss 原因；日志 `cached: N/M files`。
3. **硬门禁**：增量路径产物只有「原字节保留」与「全量重编」两态——byte-identical 天然成立；
   自举 gen1==gen2 不动点门禁路径（workspace 模式）本 change 完全不触碰。
4. **不做 indexed zpkg 模式**：本 change 只覆盖 packed 模式（cache zbc = fullMode）；
   indexed/FILE 模式继续留在 `self-hosting-future-indexed-zpkg`（无消费方，VM reader 显式 bail）。
5. **不做 workspace 增量布线**（User 裁决 2026-07-05）：workspace/flat（`outputDirOverride`）
   路径不落 cache、不 probe——布线需 WsPlan 携带 cache 目录 + xtask gen 脚本 `--no-incremental`
   纪律（否则 gen2 跳过使字节对比空洞化），整体作 follow-up change（Deferred 登记）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.pipeline/src/IncrementalBuild.z42` | NEW | Probe（hash/cache/exports 三校验，any-fresh→all-fresh + 全命中→跳过）|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `_build`：cache_dir 解析 + cache 写盘 + probe 集成 + `--no-incremental` |
| `src/compiler/z42c.project/src/ZpkgReader.z42` | MODIFY | 补消费面：MODS 头 per-file (src, hash, ns) 读取（`ReadSourceHashes`）|
| `src/compiler/z42c.pipeline/tests/incremental/` | NEW | probe 判定单测（no-record / hash-diff / no-zbc / 全命中→AllCached）|
| `src/compiler/z42c.pipeline/README.md` | MODIFY | 核心文件表 + 功能索引 |
| `src/compiler/z42c.driver/README.md` | MODIFY | `--no-incremental` 命令面 |
| `.gitignore` | MODIFY | 全局 `.cache/`（src 下 test-unit 单工程构建产 cache 防入库；Scope 追认 2026-07-05）|
| `src/compiler/z42c.project/README.md` | MODIFY | ZpkgReader 消费面扩展 |
| `docs/design/compiler/self-hosting.md` | MODIFY | Deferred `self-hosting-future-indexed-zpkg` 改写：cache/增量已落地，剩 indexed 模式 |
| `docs/design/compiler/project.md` | MODIFY | 增量编译节「对齐」刷新（z42c 实现现状）|
| `docs/roadmap.md` | MODIFY | Deferred 索引行同步 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | compiler 锁登记/释放 |

**只读引用**：`src/compiler/z42c.ir/src/BinaryFormat/{ZbcWriter,ZbcFormat}.z42`（格式 SoT，反向对照）；
`src/compiler/z42c.project/src/ZpkgBuilder.z42`（BuildPackedD 合并点）；C# 历史实现参考
`git show <remove-dotnet 前>:src/z42c/.../ZbcReader.cs`（已删，从 git 历史读）。

## Out of Scope

- indexed zpkg 模式（`pack=false` 的 `<dist>/<rel>.zbc` stripped 布局）——留 Deferred。
- **workspace / flat 模式的 cache + 增量布线**（WsPlan 携带 cache 目录 + `_build` cache
  override + xtask gen 脚本 `--no-incremental` 纪律）——follow-up change，Deferred 登记。
- VM / zbc / zpkg 格式变更：**零格式 bump**（fullMode zbc 与 packed zpkg 均为既有格式）。
- xtask / CI 消费 cache 的进一步提速编排（cache 落地后另行评估）。
- stdlib / workload / toolchain 各 toml 的 cache_dir 显式配置调整。

## Open Questions（已裁决 2026-07-05）

- [x] 混合 cached/fresh 重建（原 D2/D3）：git 历史核实 C# 最终形态已放弃该路径
  （any-fresh→all-fresh + 全命中→跳过重写）→ 采最终形态，ZbcReader 不移植，
  project.md 增量节按最终形态改写（修文档漂移）。
- [x] `.gitignore` 全局 `.cache/` 追认；workspace 布线移出本 change。
