# xtask 体系 Review 报告

> 日期：2026-07-07。范围：`scripts/` 下 xtask 全部源码、`.github/`（ci.yml / release.yml / actions）、
> xtask 相关文档（scripts/README.md、docs/book/src/dev/、docs/workflow/、docs/design/testing/）。
> 所有发现均经 file:line 级验证；已剔除两个在途 change（`redesign-xtask-test`、`simplify-xtask-deps`）
> Scope 已覆盖的内容。

---

> **落地状态（2026-07-07）**：
> - **第一节 bug** #1/#2/#4 → change `fix-xtask-review-bugs` 已修并归档；#3 复核已由 `4e8ea7d9` 提前修好。
> - **第二节代码收敛** → change `consolidate-xtask-helpers` 落地安全子集（§2.2 `_driverZpkg`、
>   §2.3 `_compilerMembers`、§2.9 注释腐烂、§2.10 小修）；§2.1 workspace-build helper + §2.5/2.6/2.8
>   结构性重构留后续独立 change（需 fixpoint/packaging 验证）。
> - **第四节文档去漂移** → change `fix-xtask-doc-drift` 落地（20 live 手册 + 4 规则文件的死命令/
>   `--scope` 幽灵/env 校正）；§4.6 design/testing 冻结 + §4.4 GREEN 清单全 SoT 收敛留后续。
> - **第三节 CI** → change `ci-hardening` 落地低风险子集（全 job `timeout-minutes`、`cargo install`→
>   `taiki-e/install-action`、bench-pr glob 修正 + Swatinem 缓存）；§3.1 JIT shard 消费工件、删
>   `bench-e2e`（可能是 required check）、§3.4 test dist 进 CI、§3.5 归档 shell 收敛留后续（需 CI 侧
>   验证 / branch-protection 确认 / User 定 gating）。

## 一、必须先修的 bug（4 个）

### 1. `test changed` 对 stdlib/toolchain 改动直接失败（pre-existing）

`scripts/test/xtask_test_changed.z42:219-221, 251` 的映射表仍输出 `xtask test lib <lib>`，
但 router 只注册了 `test stdlib`（`scripts/xtask_cli.z42:261`）。`_runPlanCmd` in-process 重入
Router → unknown → exit 2。即任何 stdlib 或 toolchain 文件变更时 `test changed` 必然失败。
不在 redesign-xtask-test 的 Scope 内（它只改了 vm→e2e 的映射）。

**修复**：四处改 `xtask test stdlib …`；建议给 `test changed` 补映射产物 dry-resolve 自检，
防止下次命令改名再度静默漂移。

### 2. release.yml 使用 3 个已删除的命令——下次打 tag 发版必挂

- `.github/workflows/release.yml:143` — `package release --rid`
- `.github/workflows/release.yml:248` — `release assemble-desktop-workload`
- `.github/workflows/release.yml:265` — `release gen-release-index`

merge-package-release（4d7dd5e2）同步了 ci.yml 却漏改 release.yml。当前命令面
（`_packageRouter`）只有 `sdk|runtime|workload|index`。

**修复**：对齐 ci.yml 写法——package job 按 RID 类别改 `package sdk / runtime / workload`；
publish job 改 `package workload <version> dist` + `package index <version> dist stable v<v> <v>`。

### 3. ci.yml Windows 腿测试覆盖清零（redesign-xtask-test 工作区落地漏改）

工作区 diff 删掉了 Windows 专属的 regen + cargo-test 两步，新注释声称 runtime stage
"run on every leg"，但唯一跑 `xtask test all` 的步骤仍带
`if: matrix.os != 'windows-latest'`（`ci.yml:221`）。结果 windows-x64 腿只 bootstrap、
0 个测试——比重构前还少。`ci.yml:257` 旁的注释已自证文件锁问题不存在。

**修复**：放开 Windows 进 `test all`（或至少加一步 `xtask test runtime`）；顺带更新
`ci.yml:205` 的陈旧理由注释（"existing scripts are bash" 已不成立）。
除这两处外，ci.yml 对本轮重构的其余迁移已核对完整一致。

### 4. bench 场景枚举未排序——踩了本仓 common-pitfalls §1

`scripts/xtask_bench.z42:49` `Directory.Enumerate` 未 sort，`--quick` 选中的"前 2 个场景"
和 e2e.json 顺序跨 OS 非确定。用现成 `_sortedStrings` 包一下即可。
同函数 `:42` 还无条件 `_buildCompiler()`（每次 bench 触发完整自举重建，分钟级），
可换成现成的 `_ensureToolchainDeps`。

---

## 二、代码简化（高杠杆项）

主线：**已有抽象没被贯彻**——helper 都在，半数调用点没用。

### 2.1 z42c driver 调用链手写 ~9 处；「gen1/gen2 同参」不变量只靠注释

stdlib / compiler-gen1 / compiler-gen2 三段 `--workspace` 构建逐字节几乎相同
（`xtask_stdlib.z42:94-98`、`xtask_compiler.z42:69-73`、`:307-311`），而
`xtask_compiler.z42:304` 注释明言 gen2 与 gen1「完全同参」——自举不动点的正确性
只靠注释纪律维持，2026-07-05 的 gen2 漂移 bug 正是这里分歧造成的。

**建议**：common 增 `_z42cWorkspaceBuild(vm, driver, cwd, libs)` 三处共用；单 toml 构建处
改用现成 `_z42cBuildToml`（`xtask_compiler.z42:185` 那次 z42.builder 构建与它等价却手写，
且其中 `.WorkingDirectory(root)` 是无效调用）。把不变量从注释升级为代码保证。

### 2.2 规范路径字面量 ~15 处绕过已有 helper

driver zpkg 完整路径手拼 ×8（stdlib:82、compiler:68/180/286、e2e:46、bench:43、
bootstrap_check:89、common:307），stdlib flat dist 手拼 ×9——而 `_libsDir` /
`_selfContainedDriverDir`（`xtask_toolchain.z42:88-90`）早已存在。最刺眼的是
`xtask_compiler_e2e.z42:50` 同一函数内已定义 `libsRel`，:188/270/274/278 四处又重新内联。

**建议**：新增 `_driverZpkg(root)` / `_memberDist(root, member)`，`_selfContainedDriverDir`
移入 common。dist 布局近期频繁变动，收敛后只改一处。

### 2.3 `_assembleDriverHome` 硬编码 7 个编译器成员，绕过 `_compilerMembers` SoT

`scripts/test/xtask_test_cross.z42:198-213` 手写成员数组，而 `xtask_stdlib.z42:140-143`
已有从 workspace toml 派生的 `_compilerMembers(root)`。新增编译器成员时这里静默漂移
（注释已互相矛盾：common 说 "6 siblings"、cross:42 说 "7 个"）。一行改动的高杠杆防护。

### 2.4 `test dist` 双重构建

`scripts/test/xtask_test.z42:136-152` 的预构建波（_buildCompiler + cargo + stdlib）与
`_buildPackageCore` → `_packageDesktop` 内部构建完全重复；包已存在路径下预构建三步全部多余。
删掉预构建波、只留 `if (!_hasReleasePkg) _buildPackageCore(...)`，`test dist` 时间近乎减半。

### 2.5 package 四平台"复制粘贴后各自演化"

- runtime-pkg 开场/收尾三处同构（ios:25-30 / android:37-43 / wasm:24-30 逐字同构）
- ABI headers 拷贝对（z42_abi.h + z42_host.h）出现 5 次
- manifest `[package]` 头 8 行拼接出现 5 次（desktop:200-206、ios:136-144、android:214-222、
  wasm:160-168、release:66-70）
- desktop RID 列表在 `xtask_release.z42` 硬编码 3 次（:35-36, 120-121, 168）

与 test 侧干净的 `IPlatformBackend` 抽象形成鲜明对比。**建议**：提取
`_runtimePkgScaffold` / `_copyAbiHeaders` / `_manifestPkgHeader` / `_desktopReleaseRids()`，
平台差异留各自文件。约 -80 行；manifest 字段增删 5 处改 1 处。

### 2.6 golden 枚举器双份 ~110 行

`xtask_test_vm.z42:275-406`（`_enumerateCasesF`）与 `xtask_test_dist.z42:362-466`
（`_enumerateDistCases`）是同构的"src/tests → src/libraries → flat"三段游走，差异只在
过滤条件（artifact `.zbc` 存在 vs source `.z42` 存在）。可统一为产出双路径 case 的
共享枚举器，两个 runner 各自按存在性过滤。约 -100 行；新增 golden 布局只改一处。

### 2.7 成对小重复

| 重复 | 位置 | 建议 |
|------|------|------|
| 两套字符串排序 | `xtask_common.z42:82-101`（`_sortedStrings` 选择排序）vs `xtask_golden.z42:39-51`（`_sortStrings` 插入排序） | 保留一个，另一个改 3 行包装 |
| 四个 zpkg 拷贝变体 | cross:261-288、package:243-254、platform:163-174 vs 现成 `_copyAll` | ✅ cross 的 `_copyZpkgs`/`_copyStdlibZpkgs` 已收敛为 `_copyAll` 薄包装（错误处理逐字一致，编译 42/42 + cross-zpkg 4/4 验证）。剩 `_stageCopyExt`（stdlib，用 `_copyIfExists` 语义需单独核）+ package/platform 变体待做 |
| 两份 `_splitLines` + JUnit writer | `xtask_test_ios.z42:115-153` vs `xtask_test_desktop.z42:94-136` | 骨架上浮 `xtask_test_platform.z42`，backend 只做行解析 |
| `_applyToolchainOpt`/`_applyVerbosityOpt` 70 行孪生 | `xtask_cli.z42:76-104` vs `:109-144` | ✅ 抽 `_stripTwoTokenOpt` + `_OptStrip` 结果类（L1 无 lambda，用小类返多值，仿 `_MapResult`）；两 pass strip 归一处，两函数改薄。编译 42/42 + verbosity/toolchain 剥离 smoke 验证 |
| 两套 TempDir API 混用 + temp 泄漏 | cross:204（`File.CreateTempDir` 从不删除）、dist:61 | driver-home 改固定 `artifacts/.scratch/` 路径 |
| 成员 zpkg verify 循环两份 + verbosity 矛盾 | stdlib:101-121（`_vDetailed`）vs compiler:113-132（无条件打印） | 抽 `_verifyMemberZpkgs`，输出统一门控 |
| `_ensureSeed` 双重解析 SDK | common:317-318 | 解析一次传入 |

### 2.8 超行数规范（函数软 40 / 硬 60；文件软 300 / 硬 500）

超硬限函数 20+ 个，最严重：

| 函数 | 位置 | 行数 | 拆法 |
|------|------|------|------|
| `_testCompilerE2e` | `xtask_compiler_e2e.z42:43-284` | ~242 | 6 个「写源+toml→build→直跑」工程表驱动化 → ~120 |
| `_regenGolden` | `xtask_test_assets.z42:29-220` | ~192 | 拆枚举（`_collectGoldenCases`）与批量编译 |
| `_testCrossZpkgImpl` | `xtask_test_cross.z42:27-190` | ~164 | 提 `_runOneCrossCase` + `_fixtureDist(dir)` helper |
| `_depsInstallAndroidSdk` | `xtask_install_android.z42:27-187` | ~161 | 按 [1]-[6] 步骤各提一函数 |
| `_packageDesktop` | `xtask_package_desktop.z42:16-167` | ~152 | 8 连发 `_z42cBuildToml` + 5 组 `_z42bPublish` 改数组循环，约 -45 行 |

文件超软限 300：`xtask_test_dist.z42` 466（逼近硬限）、`xtask_cli.z42` 460、
`xtask_common.z42` 417、`xtask_compiler.z42` 406、`xtask_test_vm.z42` 406、
`xtask_install.z42` 401、`xtask_test_changed.z42` 368、`xtask_test_lib_units.z42` 360、
`xtask_package_desktop.z42` 340、`xtask_test_cross.z42` 324。多数会随上述提取自然回落。

### 2.9 注释腐烂成体系（描述已删除的架构）

- `xtask_stdlib.z42:5-25` 文件头仍完整描述已删除的 C# 种子 5 步流程；`:59-63` 引用全仓
  零命中的 `Z42_STDLIB_CSHARP_SEED`
- `xtask.z42:93-95` 仍称 CI 保留 "raw dotnet primer"；`:211-214` helper 清单列的
  `_sh`/`_at`/`_join` 全都不存在；`:5-10` 命令树列已并入 toolchain 的 `build launcher`
- `xtask_common.z42:47-52` `_ensureDriverVm` 注释整段以已删除的 C# Driver 为存在理由
  （函数本身是否还有消费者待核实，无则连函数删）
- 多处仍称已被 z42b 取代的 "z42-test-runner"（`xtask_test_lib_units.z42:93-95, 121-127` 等）
- compiler e2e 硬编码 `/tmp` 工作目录 ×4（e2e:206/214/222/240，Windows 隐患），理由注释
  引用已删除的 C# 编译器行为；改 `Directory.CreateTempDir`

一次 docs-only commit 清理，约 -40 行误导性注释。

### 2.10 杂项小修

- `_stageToolchain` 缺配对 `_procEnd`（`xtask_stdlib.z42:267/286-287`，▶/✔ 协议破缺，1 行）
- `_utcNow` 用外部 `date` 进程（Windows 上非可执行文件），`xtask_release.z42:116` 又裸写
  一遍且无 ExitCode 检查——改调 `_utcNow` 或换 Std.Time API
- `test dist` help 文案与实际默认不符（cli:274 说 default interp，实际 interp+jit 都跑）
- `bench stdlib` 进度标签打成 "test stdlib"（`xtask_test_lib.z42:64`）
- `_pkgSha256Check` 名不副实（实际是全量字节比较，package:258）
- `_testAll()` 省略尾参的隐式调用改显式 `_testAll(false)`（cli:33）
- `_hasReleasePkg` 可由 `_latestReleasePkg` 派生（test:154-179，-10 行）
- `_regenCore` 死参数（`release` 恒 false 未用）+ `_buildStdlib` 纯转发 +
  `_ensureCompilerTooling` 单调用包装（xtask.z42:96-98/147-154、compiler:86-89）
- `test packages` 自检硬编码形状计数（count==3/9/8 + 下标断言），改包含性断言降维护成本
- `_mapFile` 的 `scripts/xtask` 前缀在子目录化后只命中顶层 2 文件（changed:257），
  改 `scripts/`
- deps check 缺 key 时以未捕获异常收场而非整洁 ✗ 报告（`xtask_deps.z42:68-71`；
  wasm 假检查本身已由 simplify-xtask-deps 覆盖）

---

## 三、CI 改进

### 3.1 8 个 JIT shard 各自全量引导（重复算力 + 自带 nightly 漂移 flake）

vm-jit（ci.yml:607-611）/ stdlib-jit（:657-661）每 shard 都跑完整 ci-bootstrap，与
toolchain-bootstrap 同构——每次 push 在 linux-x64 重复 9 次；且 :1160-1163 自认这些
download-bootstrap gate 在格式 bump 后有 self-heal 红窗。ci.yml:500-501 已写明既定方向是
downstream-consume。**建议**：改 `needs: toolchain-bootstrap` + 消费 toolchain 工件，
彻底消除这两腿的 nightly 漂移 flake（bootstrap-seed.md 的 cold-start 清单可少两项）。

### 3.2 PR bench 双跑

ci.yml `bench-e2e`（:745-747，informational）与 bench-pr.yml（gating，阈值还不一致）重叠，
每 PR 多一整条 bootstrap+bench 腿。且 `bench-pr.yml:23` 的 `scripts/xtask*.z42` glob 在
子目录化重组后只匹配顶层文件，漏掉大部分 xtask 源。**建议**：二留一（建议留 bench-pr.yml），
glob 改 `scripts/**/*.z42`，缓存统一 Swatinem。

### 3.3 `changes` job 的 vm filter 已无消费者

唯一消费者（Windows cargo-test 步）已被本次 diff 删除；`outputs.vm` 零引用，build-and-test
的 `needs: changes`（:87-90）空挂拖慢 4 条腿 ~20-30s。删 filter 段 + needs + 陈旧注释。

### 3.4 CI 覆盖缺口：包只验布局、从不验能跑

`test dist` / `test packages` 不在任何 job 中；ci.yml 用 ~150 行手写 `test -f/-d` shell
（:327-366、917-947、1008-1044、1101-1128）只验证包布局。**建议**：host-package 追加
`xtask test dist`（产物就在原地，成本 = goldens 一遍）；`test packages` 挂 ubuntu
build-and-test；布局断言逐步下沉为 xtask 的 package verify。

### 3.5 归档 shell 双实现

ci.yml publish-nightly（:1217-1294）与 release.yml（:145-194）各写一份 ~78 行 RID 归档
（rid 分类 + tar/zip 规则重复维护）。**建议**：加 `xtask package archive <label> <src> <out>`
两个 workflow 共用，与 bug #2 一并修。

### 3.6 低垂果实

- 全 ci.yml 无一处 `timeout-minutes`——挂死按 GitHub 默认 6h 计费；重型 job 加 45-60min
- test-android 的 emulator 步无重试（归档记录已认定 android-emu 是 flake 源）
- feature-matrix job（:846-875）无 Rust 缓存（每次 4 个 release 配置全冷编）且与
  `xtask feature-matrix` 命令双实现
- 4 处 `cargo install`（cargo-ndk / wasm-pack 等）冷编未缓存，换 taiki-e/install-action 秒级
- 陈旧注释：:1159（windows pkg 已迁移）、:205（bash 理由）；:621 vs :675 的
  `--jobs=4`/`--jobs 4` 风格不一

---

## 四、文档

### 4.1 15+ 处死命令：`xtask package release` / `xtask build package`

命令已删（merge-package-release），但仍是这些页的"统一入口"（全部会 exit 2）：
`docs/workflow/packaging.md:10-12`（整页）、`release.md:8-9,49`、`building/` 下
stdlib/compiler/vm/windows/wasm/android/ios 各页、`testing/verify-by-change.md:18`、
`scripts/README.md:151-160`（`build package` 流程图整节）+ `:91,209,243`、
`.claude/rules/workflow.md:511-512`。统一替换为 `package sdk [--profile debug]` /
`package runtime --rid <rid>`。

### 4.2 `--scope` / `--parallel` / `--quick` / `--with-dist` 从未在 z42 版 xtask 实现

`xtask_test.z42:185-186` 注释明言是 "a later increment"；裸 `--scope=full` 会被 shim 当
leading flag 静默吞掉。却被写成现行机制：`docs/workflow/testing/README.md:37-115`
（两大节 ~70 行）、`.claude/rules/workflow.md` 阶段 8（「commit 前必须 `--scope=full`」
按字面不可执行）、`scripts/README.md:279-280`、`docs/book/src/dev/test-gate.md:26-28,51-60,97`。
**建议**：删掉或标 Deferred；GREEN 规则改「commit 前必须完整 `xtask test`」。

### 4.3 `.claude/rules/bootstrap-seed.md` 种子解析顺序过期

仍写 `Z42C_DIR → Z42_TOOLCHAIN → Z42_HOME`、"CI 只设 `Z42_TOOLCHAIN`"，而
simplify-compiler-build 已折叠为 `Z42_HOME`（ci-bootstrap/action.yml:104,142 实际设
`Z42_HOME`）；`scripts/xtask_bootstrap_check.z42` 路径也已移到 `scripts/build/`。
规则文档写错 env 变量名比普通文档更危险——它是改种子路径时的操作依据。
改为一句话 + 链 scripts/README（现行正确口径）。

### 4.4 GREEN stage 清单 5-6 处独立维护且已互相矛盾

有的含 runtime、有的不含；`scripts/README.md` 同一文件内自相矛盾（:86/:105-120 五 stage
含 runtime ✅ vs :188/:276 四 stage 缺 runtime）；`.claude/rules/workflow.md` 阶段 8 与
`docs/workflow/ci.md:114-127` 均无 runtime 且含 `test lib`。
**建议**：SoT 定为 book test-gate.md（机制）+ scripts/README 命令表（用法）两层，
其余全部改「跑 `xtask test`，stage 组成见 test-gate.md」一句 + 链接。

### 4.5 本轮 redesign-xtask-test 文档同步的漏网

- `regen` 残留：`docs/book/src/dev/build.md:1,3,114,179-180`（标题/页头坐标/节名/实现表仍指
  `scripts/xtask_regen.z42`）、`xtask.md:66`、`dev/README.md:27`、`scripts/README.md:287`、
  `docs/workflow/quickstart.md:37`、`testing/bootstrap.md:106`
- `test lib` 残留 ~12 处：`docs/workflow/ci.md:125`、`.claude/rules/workflow.md:500,535`、
  `testing/README.md:29`、`stdlib-tests.md:75`、`building/stdlib.md:69`、
  `book/dev/test-gate.md:69-76`、`design/testing/` 多处
- `docs/workflow/testing/vm-tests.md:46`：`build test --no-stdlib`——该 flag 不存在，
  会 CliException

### 4.6 design/testing/testing.md 冻结名存实亡

声明"不再更新"的 ~1900 行旧文档仍被 5 处 workflow 页"详见"引用
（vm-tests:63,76、changed-only:30、stdlib-tests:43、unit-tests:36、testing/README:3），
本轮工作区还在被迫改它。把被引用的「目录组织/归属规则」两段迁走后断链，才能真正冻结。

### 4.7 z42-test-runner 幽灵

Rust runner 已被 z42b 取代，但 `stdlib-tests.md:3,36-40,52-53,83`（含不存在的二进制路径）、
`testing/README.md:16`、`building/stdlib.md:69`、`src/toolchain/README.md:12`、
`.claude/CLAUDE.md` 代码库结构行仍在教人用它。

### 4.8 死链 / 死路径 / 结构

- `book/dev/build.md:83,95`：`../../../.claude/rules/…` 少一层 `../`（且 book 发布后
  `.claude/` 链接本就出站失效，建议 book 内不直链）
- `docs/workflow/README.md:50`：`artifacts/build/z42c/` 已更名 `artifacts/build/compiler/`
- `docs/design/runtime/zbc.md:463`：不存在的 `generate-fixtures.sh`（同页 :430 已写对）
- `book/dev/packaging.md:72`：`release assemble-desktop-workload`（命令已删）
- 结构：`vm-tests.md` + `cross-zpkg.md` 两页对齐的是旧命令面（现已合并为 `test e2e`），
  建议合并为 `e2e-tests.md`；`bootstrap.md` 与 `ci.md` 是"CI 拓扑双胞胎"且现状描述互相矛盾
  （bootstrap.md:62-64 说测试 job 仍各自引导，ci.md/ci.yml 已是 --no-build 消费），
  拓扑与现状只留 ci.md 一份
- 小项：`quickstart.md:36` gate 缺 runtime；`workflow/README.md:17` / `quickstart.md:37`
  的 `./xtask help` 不是命令（应 `-h`）；`verify-by-change.md:15` / `unit-tests.md:16` 教裸
  `cargo test`（会踩并发 SIGSEGV race，应推 `xtask test runtime`）

---

## 五、流程建议

四份 review 的 stale 引用问题同根源：**命令面重构的文档半径系统性被低估**
（merge-package-release 漏了整个 `docs/workflow/`，redesign-xtask-test 已同步 8+ 文件
仍漏十余处）。建议把一条机械检查写进 workflow.md 阶段 9 的 doc-check 清单：

> **删/改任何 xtask 子命令时，`grep -rn "<旧命令>" docs/ scripts/ .claude/` 必须清零。**

本次 review 正是靠这一招抓到全部漏网。

---

## 建议的落地顺序

1. **随 redesign-xtask-test 收尾一起修**（都是它的漏网）：`test changed` 的
   `test lib`→`test stdlib`、ci.yml Windows 腿、`regen`/`test lib` 文档残留
2. **独立 fix change**：release.yml 死命令（发版硬故障）、bench 未排序枚举
3. **一个 refactor change 做代码收敛**：路径/构建 helper 贯彻（§2.1-2.3）+ package 四平台
   scaffold 提取（§2.5）+ 超限函数拆分（§2.8），预估净删 300+ 行
4. **一个 CI change**：JIT shard 改消费 toolchain 工件 + bench 双跑二留一 +
   timeout/缓存低垂果实
5. **一次 docs-only 清理**：死命令批量替换 + GREEN 清单收敛 SoT + bootstrap-seed.md 校正 +
   注释腐烂清理

## 整体评价

xtask 的骨架是健康的——common 三件套、toml 驱动的 SoT 意识、test 侧 `IPlatformBackend`
抽象、CI 的 compile-once 拓扑方向都很好，注释里对设计决策的留痕质量罕见地高。
债务集中在「快速迭代期的收尾没扫干净」：新 helper 落地后旧调用点不回迁、命令面改名后的尾巴
（映射表 / help 文案 / 文档）、大重构改了代码没清注释。没有发现架构性问题。

---

## 附录 A：z42c 编译走 JIT 的加速机会（2026-07-11 实测，非原 review 项）

> **✅ 已落地（2026-07-12，change `consolidate-z42c-invocations` B 步）**：`_z42cMode()` 默认
> `interp`→`jit`。前置全达成——jit-fixpoint-check.yml **4 平台全绿**（run 29168922905：linux-x64 /
> linux-arm64 / windows-x64 / macos-arm64 的 z42c workspace 编译 interp==jit 逐字节一致）+ User 拍板
> 接受信任基线移到 cranelift/JIT + toolchain 锁在手 + 非格式-bump 窗口。逃生舱 `Z42C_BUILD_MODE=interp`
> 永久保留。机制落 `docs/book/src/dev/build.md`「z42c 编译执行模式」节。下方为落地前的调研记录。

**背景**：xtask 里 **18 个 z42c driver 调用点全部硬编码 `--mode interp`**（`.Arg("--mode").Arg("interp")`，0 处 jit），
所以 CI 的 `build compiler` / `build stdlib` / golden 重生**全走解释执行**。而 z42vm 默认已是 JIT
（`make-jit-default` 2026-06-20，`src/runtime/src/main.rs:578`），z42c 也早已 JIT-capable
（`fix-jit-cross-zpkg-call` 2026-06-20，byte-identical）。问题：z42c 编译改走 JIT 能否加速 CI？

**实测**（本地 macOS，同一 warm `z42c.driver.zpkg`，仅切 `--mode`）：

| 场景 | interp | jit | jit 相对 | 产物 |
|---|---|---|---|---|
| 大编译：`build z42.core --no-incremental`（72 文件） | 18.65s ± 0.09 | **5.22s** ± 0.06 | **3.6× 快** | 逐字节相同 ✓ |
| 小编译：`--emit-zbc <小文件>`（每次全新进程，含 z42c 自身 JIT warmup） | 2.56s ± 0.02 | **1.53s** ± 0.02 | **1.67× 快** | 逐字节相同 ✓ |

**关键发现**：

- **JIT 两个场景都更快**——大编译 3.6×，海量小编译（golden 逐 case spawn 那种）**仍 1.67× 快**。
  小编译那 1.5–2.5s 主要是 z42c **进程启动 + 加载 7 包 + stdlib**，JIT 把这段热路径也加速了，
  **warmup 成本远小于收益**。→ 原先"JIT warmup 会拖慢小编译、应大编译切/小编译留"的顾虑被数据**推翻**，
  很可能是**全切**。
- **产物两样本 byte-identical**（`z42.core.zpkg` + 小文件 zbc）——对"JIT 不破自举不动点"是好兆头。

**切换前置清单（速度已明确赢，剩下纯粹是确定性/信任验证——切换前必须逐项达成）**：

1. **全平台 × 全包不动点 byte-identity**：7 个 z42c 包、全 stdlib、全 golden，在
   linux-x64 / linux-arm64 / macos / windows 上都 JIT 编与 interp 编逐字节一致。cranelift 是另一条
   codegen 路径，单机一致 ≠ 跨平台保证（HashMap/浮点/顺序敏感角落，见 common-pitfalls §1）。
   - **✅ macos-arm64 已本地验证（2026-07-11，干净 0.30 树）**：`z42c build --workspace`（canonical
     per-member）分别 `--mode interp` 与 `--mode jit` 编 z42c **7 包全部 byte-identical**（除末尾 16B
     BLID 内容哈希；core/ir/syntax/project/semantics/pipeline/driver 逐字节相同、size 全同）。
     另 z42.core（stdlib）+ 小文件 emit 亦 byte-identical。→ **JIT 不破自举不动点，此平台确认。**
   - **✅ 全平台已验证（2026-07-12）**：`jit-fixpoint-check.yml`（run 29168922905）在 linux-x64 /
     linux-arm64 / windows-x64 / macos-arm64 四平台确认 z42c workspace 编译 interp==jit 7 包逐字节一致。
     另本地 `test compiler`（jit 默认）不动点 7/7 gen1==gen2 + 19 units + e2e 全绿。
2. **「interp = 可信参考」的权衡**：不动点只查 gen1==gen2 一致，**查不出"JIT codegen bug 让两代错得一样"**。
   interp 更简单、更被信任。换 JIT = 把信任基线移到较新的 cranelift 路径——属 User 权衡的设计决定。
   - **✅ User 已拍板（2026-07-12）接受移到 cranelift/JIT**（4 平台字节一致 + 逃生舱 interp 兜底为据）。
3. **时机**：需 toolchain 锁释放 + 格式 bump/WIP 落定的稳定期做（避免红了分不清是 JIT 还是格式引起）。

**改动面**（前置达成后）：`scripts/` 里 18 个 `.Arg("--mode").Arg("interp")` → `jit`（toolchain 子系统，
需占锁）。粗估 CI 收益：z42c 编译占 CI 相当一块，1.67–3.6× 打下去，每 run 省几分钟量级。

> 落地方式待定：可能不是简单全改字面量，而是给 z42c 调用抽一个 `_z42cRun(vm, driver, mode)` helper
> + 一个 `Z42C_BUILD_MODE` 环境/开关（默认 jit，格式 bump 期可临时回退 interp），与 §2.1 的
> `_z42cWorkspaceBuild` 收敛一起做最自然。
