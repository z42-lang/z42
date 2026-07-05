# Design: z42c 增量编译 cache 移植

## Architecture

```
z42c build <toml> [--release] [--no-incremental]        （单工程模式；workspace 不走）
  ├─ _resolveCacheDir(pm, projectDir)            ← 镜像 _resolveDistDir 级联
  ├─ IncrementalBuild.Probe(srcs, texts, projectDir, cacheDir, lastZpkg)   ← 新（z42c.pipeline）
  │    ├─ ZpkgReader.ReadSourceHashes(lastZpkg)  ← 扩展：MODS 头 per-file (src, hash, ns)
  │    ├─ 每文件：hash== ∧ cache/<rel>.zbc 存在 ∧ TSIG 含 ns → hit
  │    ├─ 记录数 != 当前源文件数 → 全 fresh（防「删文件后全命中保留陈旧模块」）
  │    └─ any miss → AllFresh；全 hit → AllCached
  ├─ AllCached ∧ lastZpkg 存在 → 跳过重编/重写（保留现有 zpkg；exe 仍复制依赖）→ return
  └─ 否则全量：BuildPackage → 逐文件 ZbcWriter.Write → cache/<rel>.zbc → BuildPackedD → dist
```

## Decisions

### D1: cache 目录级联与缺省名
**问题**：cache 落哪、叫什么。
**决定**：镜像 [project.md L3](../../../design/compiler/project.md) 既有规范——
`[build].cache_dir`（支持 `${output_dir}` 模板）→ 缺省 `${output_dir}/.cache` → 无 [build] 时
`<projectDir>/.cache`；workspace member 继承 `[workspace.build].cache_dir`（compiler workspace 已
显式配 `${output_dir}/cache`，output_dir 模板含 `${project_name}` 无碰撞）。SourceDiscovery 已
排除 `.cache/`（[SourceDiscovery.z42:69](../../../../src/compiler/z42c.project/src/SourceDiscovery.z42#L69)），
显式配非隐藏名 `cache` 时其位于 artifacts 树、不在源码 glob 范围，同样安全。

### D2: probe 采 C# 最终形态——any-fresh→all-fresh + 全命中→跳过（User 裁决 2026-07-05）
**问题**：原设计假定混合 cached/fresh 重建（ZbcReader 重建 cached CU）；git 历史核实
C# 当天即放弃该路径（`ed901f01`：clean 720/720 → no-change 增量重建 6/720 失败——
per-namespace TSIG 重写丢元数据 + fresh CU 新 string-pool/impl 与 cached 合并不一致）。
**决定**：镜像 C# 最终形态。任一 fresh → 整包全量重编（正确性零风险）；100% 命中 →
**什么都不写**（上次 zpkg 完整在盘、无新信息）。ZbcReader / cachedExports 注入 /
Usings 重建全部不需要。增量收益主场景 = 多包构建里未改动的包秒跳过。
**补强（超出 C# 的一个防御）**：zpkg MODS 记录数 != 当前源文件数 → 全 fresh，
堵「删源文件后其余全命中 → 跳过保留含已删模块的旧 zpkg」的洞（C# 同洞未堵）。

### D3: exe 跳过时仍执行依赖复制
z42c `_bundleExeDeps`（C# 无此层）把兄弟包复制进 exe dist；上游兄弟包变化不会使本包
源码 fresh（跨包失效跟踪 = C# 注释里的 future (b)，不在本 change）。跳过路径**仍跑**
依赖复制（幂等、廉价），保证 dist 内兄弟 zpkg 始终最新——上游变化至少在部署面不陈旧。

### D4: byte-identical 天然成立
增量路径只有两态：原字节保留（跳过）或全量重编（与 --no-incremental 同路径）。
验收：no-touch rebuild 保留原 zpkg（`cached: N/N` + 文件未重写）；touch 后重编产物与
`--no-incremental` 全量 build 逐字节相等。

### D5: 不引入 indexed 模式 / 零格式 bump
cache zbc 用 fullMode（STRS/TYPE/SIGS/EXPT/IMPT 全段，`ZbcReader.Read` 单文件即可恢复
IrModule）——`--emit-zbc` 已产同格式，Rust golden 有既有基线。indexed zpkg（stripped zbc）
无消费方，继续 Deferred。zbc/zpkg 版本号不动，无 version-bumping checklist 触发，
无 bootstrap 两-nightly 分阶段需要（不新增语法/格式，z42c 源码不使用新 stdlib API）。

## Implementation Notes

- `ReadSourceHashes`：只读 MODS 头（ns/src/hash 均为 STRS 池索引 + 五个 len+体跳过），
  复用 `ReadModuleSigs` 已验证的游标走法。
- rel 键 = 源文件相对 projectDir 路径（`.z42` → `.zbc`，与 C# `Path.ChangeExtension` 同义）；
  子目录源文件需 `Directory.Create` 中间目录。
- 源文本读一次复用（probe hash 与编译共用 texts；顺带消除 DepScan usings 聚合的二次 ReadAllText）。
- Probe 失败态全部安全回退 fresh（zpkg 不可读 / cache 缺失 = miss，不报错）。
- miss 原因枚举沿用 C#：no-record / hash-diff / no-zbc / no-export-mod（Z42_INCR_DEBUG=1 可见）。

## Testing Strategy

- 单元（z42c.pipeline/tests/incremental/）：真实 build 产物上的 Probe 四态判定
  （全命中 AllCached / hash-diff / no-zbc / 记录数不符）+ ReadSourceHashes 面。
- 端到端（阶段 5 手工脚本验证）：① 全量 build → ② no-touch rebuild `cached: N/N` 跳过、
  zpkg 未重写；③ touch 单文件 → 全量重编、产物 == `--no-incremental` build 逐字节；
  ④ 删 cache 目录 → 全 fresh 正常。
- GREEN gate：`xtask test`（vm / cross-zpkg / lib / compiler 自举 7/7 byte-identical）全绿。
