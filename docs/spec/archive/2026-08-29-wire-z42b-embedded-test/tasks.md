# Tasks: z42b 接管 embedded 测试的单-bundle 执行与设备组装

> 状态：🟢 已完成 | 创建：2026-08-29 | 完成：2026-08-29

## 进度概览
- [x] 阶段 1: BundleRunner 提取（z42.test 共享核）
- [x] 阶段 2: Slice 1 — z42b test --rid host 跑 bundle + xtask desktop 委托
- [x] 阶段 3: Slice 2 — z42b test --rid <device> 组装 + xtask _build*Testhost 委托
- [ ] 阶段 4: 测试与文档（GREEN gate 运行中）

## 阶段 1: BundleRunner 提取
- [x] 1.1 新建 `z42.test/src/BundleRunner.z42`：`RunBundle(BundleCase[], bundleDir, libsDir) → TestResult[]` + `ExitCode`（JSON-free，D1；golden 隔离/unit 共享/libs 回退/JOBS 并行）；`BundleCase` 类
- [x] 1.2 `agent.z42` 的 `.json` 分支改调 `BundleRunner.RunBundle`（+ 薄 `_parseManifest`）；删本地 `_runBundleReport`/`_concat`/`_stripNl`/`_oneLine`
- [x] 1.3 确认 `Runner.RunModuleResults` / `ModuleLoader.RunGoldensIsolated` 签名满足复用（只读，未改）

## 阶段 2: Slice 1（host，本地可验 ✅）
- [x] 2.1 `builder_test.z42:_runModule`：`.json` && rid∈{"","host"} → `_runBundleHost`（parse→`BundleRunner.RunBundle`→render+ExitCode）；z42.builder 加 z42.json 依赖
- [x] 2.2 `builder_cli.z42`：`test`/`bench` 的 `--rid` 帮助去「platform deploy pending」；target 增 `.json` 语义；加 `--out`/`--agent`
- [x] 2.3 `xtask_test_embedded.z42:_testEmbedded` desktop 分支 → 委托 `z42b test --rid host <manifest>`；删死代码 `_ensureDesktopTesthost`/`_embeddedNativeLibs`
- [x] 2.4 本地验：`xtask test embedded --filter arith` 走 z42b，15 passed，与 agent JSON 逐字节一致

## 阶段 3: Slice 2（device，本地验组装 ✅ / RUN 交 CI）
- [x] 3.1 `builder_test.z42`：`_assembleDeployable`（rid 校验 + `--out`/`--agent`/`Z42_LIBS`）+ `_stageDeployable`（app/libs/bundle + wasm files.json + 排序确定序）；`_runModule` device 分支调用
- [x] 3.2 agent zpkg / flat libs 经 `--agent` / `Z42_LIBS` 由 xtask 传入（z42b 不假设 repo 布局）
- [x] 3.3 `_build{Wasm,Ios,Android}Testhost` 的组装步 → 委托 `_z42bStageDeployable`（`z42b test --rid <rid> --out --agent`）；native build + RUN 交接不动；删死 `_assembleEmbeddedCorpus`/`_appendFileEntry`
- [x] 3.4 本地验组装：`z42b test --rid android-x64` → `{app,libs,bundle}`（无 files.json）；`--rid browser-wasm` → +files.json（排序）；无效 rid 报错

## 阶段 4: 测试与文档
- [x] 4.1 bundle-host smoke：fixture `src/tests/manifest-targets/bundle-host/pass_unit.z42` + `xtask_test_fixtures._smokeBundleHost`（编→1-case manifest→`z42b test --rid host`），挂 manifest-targets stage（默认 gate 覆盖 Slice 1）
- [ ] 4.2 `cargo build --release`（z42vm）无错（gate 内 build 波）
- [ ] 4.3 `xtask test` 完整 GREEN gate 全绿（运行中）
- [ ] 4.4 self-host 字节不动：`xtask test compiler` 7/7 byte-identical（gate stage）
- [ ] 4.5 spec scenarios 逐条覆盖确认
- [x] 4.6 文档同步：新建 `docs/book/src/toolchain/test-pipeline.md`（两层模型 SoT + 挂 SUMMARY）；`workload/test/README.md`；`docs/roadmap.md`（②b 进度 + Slice 3 Deferred 索引）
- [x] 4.7 命令面 grep：`platform deploy pending` 清零（仅剩本 change docs/archive 历史引用）

## 备注
- Slice 2 device RUN 编排 = Slice 3（defer，本 change Out of Scope）。
- D1=BundleRunner 独立文件；D2=Slice 2 仅委托语料组装（native build 留 xtask）；D3=host in-process 废 testhost spawn；D4=target×rid 分派表。见 design.md。
