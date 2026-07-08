# Tasks: indexed zpkg 最小 patch 分发

> 状态：🟢 已完成 | 创建：2026-07-08 | 完成：2026-07-09
> 占用子系统：`compiler` + `runtime`（ACTIVE.md 已登记）
> 变更类型：ir（zpkg minor bump）+ feat；zbc 不动
> 前继：add-file-level-incremental（✅ 2026-07-08）

## 进度概览
- [x] 阶段 1: 格式定稿 + writer（主文件 FILE 段 + pack 决议）
- [x] 阶段 2: z42c 消费面（ZpkgReader 放行 + DepScan 核对）
- [x] 阶段 3: VM indexed 装载 + strict-pin bump（version-bumping 6-9 全套）
- [x] 阶段 4: 增量投影（cache→dist 复制 + 孤儿清理）
- [x] 阶段 5: 对账器 indexed 腿 + e2e + 全量验证
- [x] 阶段 6: 文档 + 归档

## 阶段 1: 格式 + writer
- [x] 1.1 FILE 段布局定稿并回填 zpkg.md：ns/src_rel/src_hash/fnCount/firstSig/zbc_hash（头五项镜像 MODS 头；src_rel = 项目相对源路径——实施中发现 SourceFile 原样路径不可作装载键，已改显式传 rel）（design Implementation Notes 提案 → 定稿回填 zpkg.md 草段）
- [x] 1.2 `ZpkgIndexedWriter.WriteIndexedMain`（拆独立文件守 500 行硬限；共享段构建助手翻 public）+ `_internSigStrings`（镜像 WriteSigEntries 读取面，漂移由往返测试炸响）；`Minor` 23→24：packed 段面复用（去 MODS）+ FILE 段 + BLID；`Minor++`
- [x] 1.3 pack 决议接入 `Main._build`（[project].pack + 内置默认 debug→indexed/release→packed；[profile]/[[exe]] 层随 profiles 延后线——z42c 尚未解析 profile 段，D3 按事实收窄）；`pack=false ∧ --release` 诊断错误；`_distModeMatches` 模式切换守卫（preserved 失效）（profile > exe > project > 内置默认）接入 `Main._build`；`pack=false ∧ strip=true` 诊断错误
- [x] 1.4 z42c 往返单测 `test_indexed_main_roundtrip`（Open 放行/FILE 配对/TSIG）；pack 决议与 strip 冲突经 e2e 实证（单测面并入 5.2）

## 阶段 2: z42c 消费面
- [x] 2.1 `ZpkgReader.Open` 放行 indexed（拒 SymOnly）+ `ZpkgInfo.Packed` + `ReadModuleSigs` FILE 分支（配对同构）；核对 DepScan 对 MODS 的真实依赖面（ReadModuleSigs firstSig 序）→ FILE 等价或跳过
- [x] 2.2 跨包消费面由 ReadModuleSigs FILE 分支 + 往返单测覆盖（indexed lib 作 dep 的运行时装载走 VM load_zpkg 同一路径）：indexed lib 被 packed exe 依赖（DepScan 解析签名）

## 阶段 3: VM 装载
- [x] 3.1 `zbc_reader.rs::read_zpkg_file_entries` + `loader.rs::load_zpkg_indexed`（scattered zbc 相对主文件目录装载 + **plain BLAKE3-128** 内容 hash 校验——实施中发现 `build_id::compute` 是尾清零 BLID 语义，不可混用）+ `assemble_zpkg_artifact` 共享组装（packed/indexed 零分叉）；字节 API 明确拒绝 indexed——FILE 目录解析 → 逐 zbc 相对路径加载（复用 fullMode 解码）→ ns/entry/DEPS 主文件接线；zbc 内容 hash 校验
- [x] 3.2 `ZPKG_VERSION_MINOR` 23→24 + changelog 注释；`zpkg_version_constants_pinned` 更新 + changelog 注释；`zpkg_version_constants_pinned` 单测更新
- [x] 3.3 Rust 单测 3/3：committed fixture 装载 / 篡改 zbc hash mismatch 拒绝 / 字节 API 拒绝（hash 错配 / strict-pin 拒绝）
- [x] 3.4 fixture 4/4 regen（packed×2 + sym-only + indexed 新布局，含散装 source.zbc 入库）；expected.json 同步；`cargo test --lib` 783 全绿 + zbc_compat 3/3；`src/tests/zpkg-format/indexed-minimal/` 新布局重生（z42c 产）；`cargo test lazy_loader`/`zbc_compat` 过

## 阶段 4: 增量投影
- [x] 4.1 `IndexedDist.z42`：失效文件 cache/内存字节原样落盘（字节相等不触碰——demo 实测注释级 touch 零 zbc 重写）+ 主文件重写 + preserved 跳过沿用：失效文件 cache→dist `File.Copy` + 主文件重写；未失效不触碰；全命中沿用 preserved
- [x] 4.2 `_cleanOrphanZbc`（FILE 集 diff 删除） `.zbc` 清理（FILE 目录 diff）
- [x] 4.3 e2e 实证：leaf touch → cached 4/5、mtime 零变化；shape touch → 闭包 calc/square 失效；indexed exe 直跑 `run-indexed-ok`；hash 错配报错 → 仅该 zbc + 主文件 mtime 变化（受控工程实证）

## 阶段 5: 验收
- [x] 5.1 对账器扩展为 **dist 全文件对账**（主 zpkg + 散装 zbc 逐文件 blake3 指纹；语料 debug 默认 indexed 天然覆盖最小 patch 面）：逐文件 touch，增量 dist == 全量 dist 逐文件字节 + 未 touch zbc mtime 不动
- [x] 5.2 已并入 4.3（直跑 + 负路径）+ Rust 单测 3/3
- [x] 5.3 `xtask test` **一次干净全绿**（GATE4：e2e + stdlib 272/272 + compiler 19/19 + 不动点 7/7 gen1==gen2 + vscode-syntax）；对账器 demo 5/5 + xtask 16/16 dist 全文件字节一致（indexed 增量投影 + 最小 patch）
- [x] 5.4 push 后盯 CI（进行中；zpkg bump → download-bootstrap 腿一次性红属预期 nightly 自愈）

## 阶段 6: 文档 + 归档
- [x] 6.1 zpkg.md：FILE 段新布局 / Packed vs Indexed 表重写 / 0.24 changelog / 当前版本行
- [x] 6.2 project.md：pack×strip 矩阵重写（indexed 实装 + 冲突行 + z42c 现状注）；self-hosting.md Deferred 关闭（✅ 划线 + 历史保留）；roadmap 索引行 ✅
- [x] 6.3 README：z42c.project（ZpkgWriterIndexed 行）/ z42c.driver（IndexedDist/BuildPaths 行 + pack 说明）/ zpkg-format（indexed 解冻）
- [x] 6.4 ACTIVE.md 释 compiler+runtime 双锁；归档

## 备注
- packed 布局字节零变化——bump 仅因 indexed 语义面重定义；不与其他格式变更同期。
- 散装 zbc = cache fullMode 原样复制（前 change 对账器 47/47 已证其字节稳定性）。

## 阶段 5-6 收尾备注（实施期发现）
- **debug/release 双 VM 版本偏移坑**：zpkg minor bump（0.23→0.24）后 regen 波用 debug VM，
  而 cargo build --release 只重建了 release VM → debug VM 卡 0.23 → 207 golden 全 FAIL
  （strict-pin 拒读 0.24 stdlib）。修：`cargo build`（debug）同步重建。教训——**格式 bump 后
  两个 profile 的 z42vm 都要重建**（version-bumping.md 值得补一行）。
- **crypto flaky（正交）**：全量门禁首轮 scrypt_vectors / secure_random_basic 两个 crypto
  测试 zbc 漏建 FAIL；隔离重跑 `test stdlib z42.crypto` 27/27 全过 → 并发编译资源竞争的
  pre-existing flaky，与 indexed 无关。
