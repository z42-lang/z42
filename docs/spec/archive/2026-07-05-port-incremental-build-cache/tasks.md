# Tasks: z42c 增量编译 cache 移植

> 状态：🟢 已完成 | 创建：2026-07-05 | 完成：2026-07-05
> 占用子系统：`compiler`（已释放）
> 变更类型：feat(compiler)（恢复 C# C5 能力；零格式 bump、VM 零改动）

## 进度概览
- [x] 阶段 1: cache 落盘（物质基础）
- [x] 阶段 2: ZbcReader 移植（裁决删除——C# 最终形态无消费方）
- [x] 阶段 3: ZpkgReader 消费面扩展（ReadSourceHashes）
- [x] 阶段 4: 增量 probe + build 集成（+ 单测 6/6）
- [x] 阶段 5: 端到端验证 + xtask test 全绿 + 文档 + 归档

## 阶段 1: cache 落盘
- [x] 1.1 `Main.z42` `_resolveCacheDir(pm, projectDir, isRelease)`：cache_dir（`${output_dir}` 模板）→ `${output_dir}/.cache` → 无 [build] 时 `<projectDir>/.cache`。**workspace 继承未做**：workspace/flat 模式（outputDirOverride 非空）暂跳过 cache——WsPlan 不携带 cache 目录，布线属 Scope 外（WorkspaceBuild.z42），已列为裁决点；顺带保证 gen1==gen2 字节门禁路径零扰动
- [x] 1.2 `_build` 逐文件 `ZbcWriter.Write(irm).ToBytes()` → `cache/<rel .z42→.zbc>`（`_writeCacheZbc`，含中间目录创建；`cache -> <dir> (N files)` 单行 stderr 摘要）
- [x] 1.3 冒烟全过：launcher（output_dir 缺省级联）→ `artifacts/build/toolchain/launcher/.cache/` 5 zbc ✓；xtask（显式 `${output_dir}/.cache` 模板 + 子目录源）→ 40 zbc 含 build/common/install 子目录 ✓；无 [build] 工程 → `<projDir>/.cache/main.zbc` ✓；产物 zpkg 与改动前 **byte-identical** ✓；`xtask test compiler` 全绿（e2e 6/6 + 不动点 7/7）✓。附带：`.gitignore` 补全局 `.cache/`（test-unit 单工程构建会在 src 下产 `.cache/`，原 Scope 未列 → 待 Scope 追认）

## 阶段 2: ZbcReader（z42c.ir）——【裁决删除 2026-07-05】
- [x] ~~2.1/2.2 ZbcReader 移植~~ 采 C# 最终形态后混合重建路径不存在，ZbcReader 无消费方 → 不移植

## 阶段 3: ZpkgReader 扩展（z42c.project）
- [x] 3.1 `ReadSourceHashes`：MODS 头 per-file (src, hash, ns)（复用 ReadModuleSigs 游标走法）+ `ZpkgSourceHash` 类型
- [x] ~~3.2 ExportedModules / Dependencies 重建面~~ 随混合重建路径删除；TSIG ns 校验复用既有 `ReadTsig`

## 阶段 4: probe + 集成（z42c.pipeline / driver）
- [x] 4.1 `IncrementalBuild.z42`：Probe（no-last-zpkg / unreadable / record-count / no-record / hash-diff / no-zbc / no-export-mod）→ any-fresh→all-fresh；全命中→AllCached
- [x] 4.2 `_build` 集成：probe 前置于 DepScan（跳过时省扫描）；AllCached → 保留现有 zpkg + exe 依赖复制（design D3）+ `no changes; preserved` 后 return；texts 一次读入三处共用
- [x] 4.3 `--no-incremental` flag（workspace/flat 调用点固定 true）+ `Z42_INCR_DEBUG=1` 诊断 + `cached: N/M files` stderr 日志 + usage
- [x] 4.4 probe 单测 `z42c.pipeline/tests/incremental/` 6/6：全命中 / hash-diff / no-zbc / no-last-zpkg / record-count / ReadSourceHashes（实施中发现 `BuildModuleD` 不产 Exported——单模块路径 TSIG 用独立 `IrDump.ExtractExports`）

## 阶段 5: 验证 + 文档
- [x] 5.1 e2e 验收（launcher 实测）：fresh → `cached: 0/5` 写 zpkg+cache；no-touch → `cached: 5/5` 跳过（字节+mtime 不变）；touch → 整包重编且与 `--no-incremental` 产物**逐字节相等**；删 cache → no-zbc 全量回退；恢复后再次 5/5 跳过
- [x] 5.2 `xtask test` 全绿：✅ GREEN — all stages passed（vm + cross-zpkg + stdlib + compiler 自举不动点 7/7 gen1==gen2；e2e 内可见增量生效 `cached: 1/1` 跳过）
- [x] 5.3 文档：project.md 增量节按最终形态改写 + `incremental-future-workspace-wiring` Deferred；self-hosting.md `self-hosting-future-indexed-zpkg` 范围收窄；roadmap 索引 2 行；z42c.pipeline / z42c.project / z42c.driver README；book dev/build.md 增量小节（对齐 2026-07-05）
- [x] 5.4 ACTIVE.md 释放 compiler 锁；归档

## 备注
- 前置事实：`--emit-zbc` 已产 byte-identical fullMode zbc（writer 就绪）。
- **⚠ 规范偏差（2026-07-05，实施中发现，待 User 裁决）**：git 历史核实 C# 最终形态
  （`ed901f01` fix-incremental-build）与本 spec/design 及 project.md 增量节均不符——
  ① probe 是「any fresh → 整包失效」（混合 cached/fresh 重建因正确性问题被放弃：
  clean 720/720 → no-change rebuild 6/720 失败）；② 100% 命中 → **完全跳过重写**
  （"no changes; preserved existing zpkg"），ZbcReader 消费路径在 C# 中实际不可达。
  ⇒ 若采纳最终形态：阶段 2（ZbcReader）删除、阶段 3 缩为 ReadSourceHashes、
  design D2/D3 作废、spec「单文件改动仅该文件 fresh」场景改写、project.md 增量节
  同步改写（该节描述的旧形态 C# 从未以此形态定型 = 既有文档漂移）。
- 阶段 1 已按两形态共同前提完成（cache 落盘物理层完全一致）。
