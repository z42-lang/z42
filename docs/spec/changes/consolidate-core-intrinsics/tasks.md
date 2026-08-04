# Tasks: 去重 cross-cutting intrinsic 到 z42.core（A1）

> 状态：🟡 实施完成，待 PR/CI 全绿 | 创建：2026-08-03
> 类型：refactor + 一处 bootstrap 脚本修正。总纲：improve-stdlib-org-perf 相位 A1。

## 进度概览
- [x] 阶段 1: core 两门面（BitConverter + Clock）
- [x] 阶段 2: 时钟调用方切换（安全 (a)）
- [x] 阶段 3: 位转换调用方切换 + bootstrap 脚本修正（(c)）
- [x] 阶段 4: 文档同步
- [x] 阶段 5: 本地验证（warm）—— 见下；full gate + byte-identity 待 PR/CI

## 阶段 1: core 门面
- [x] 1.1 `z42.core/src/BitConverter.z42`：`Std.BitConverter`（4 extern，唯一声明点）
- [x] 1.2 `z42.core/src/Clock.z42`：`Std.Runtime.Clock`（2 extern）

## 阶段 2: 时钟调用方
- [x] 2.1 DateTime.z42 → Clock.WallMillis
- [x] 2.2 Stopwatch.z42 → Clock.MonoNanos
- [x] 2.3 Environment.z42 GetCurrentTimeMs 保签名委托 WallMillis
- [x] 2.4 HttpClient.z42 → Clock.WallMillis（`using Std.Runtime;` 薄面，避 Std.Time.Stop 冲突）
- [x] 2.5 Bencher.z42 → Clock.MonoNanos
- [x] 2.6 行为验证：DateTime.UtcNow>0 + Stopwatch.Elapsed>=0（fresh stdlib，均 true）

## 阶段 3: 位转换调用方 + bootstrap
- [x] 3.1 BinaryWriter.z42 → BitConverter.*（删 local extern）
- [x] 3.2 BinaryReader.z42 → BitConverter.*（删 local extern）
- [x] 3.3 ZbcInstr.DoubleToBits 保签名、委托 core
- [x] 3.4 ZbcReaderInstr.BitsToDouble 保签名、委托 core
- [x] 3.5 `_ensureBootstrapZ42Ir`：z42.ir 单包重建前先建当前源 z42.core
- [x] 3.6 grep 确认单一声明点（bit-op 仅 BitConverter.z42；clock 仅 Clock.z42）

## 阶段 4: 文档
- [x] 4.1 z42.core/src/README.md 功能索引 + 依赖表 + 设计原则（两层模型）
- [x] 4.2 src/libraries/README.md 单一声明点纪律：位转换/时钟已收敛
- [x] 4.3 organization.md「现状」表：z42.time 改 ❌ 纯脚本（时钟经 core Clock）
- [x] 4.4 self-hosting.md 轴 ④：`_ensureBootstrapZ42Ir` 现预建 core（A1 扩展段）

## 阶段 5: 验证
- [x] 5.1 cargo build z42vm（release）—— exit 0
- [x] 5.2a stdlib 全量 workspace build（25/25 member 编译通过，delegations + 跨 zpkg 解析 OK）
- [x] 5.2b 行为 smoke：位转换 round-trip（3.140625 / 2.5 精确回还）+ 时钟（wall/mono 均 true）
- [x] 5.2c z42c warm 自建成功（7/7 z42c.*；z42.ir 委托 DoubleToBits 在 z42c 运行期工作）
- [x] 5.2d bootstrap 修正隔离验证：老 core 编 z42.ir → `undefined: BitConverter`（复现）；
       预建当前源 core 后 → 通过（证明 _ensureBootstrapZ42Ir 修正必要且有效）
- [ ] 5.3 完整 `xtask test` gate（e2e / cross-zpkg / stdlib [Test] dogfood / compiler / vscode-syntax）
       —— fresh worktree 未跑端到端；**PR/CI 执行**（含 cold 冷启动腿）
- [ ] 5.4 byte-identical 自举不动点（gen1==gen2）—— 语义未变、z42c 源未变，correct-by-construction；PR/CI 确认
- [ ] 5.5 归档 + PR（合并前 rebase main + 重跑完整 GREEN）

## 备注
- 验证策略遵循 design.md：warm 路径本地验（已做），cold + full gate + byte-identity 以 PR/CI 为准。
- 本地工具链：worktree cargo z42vm + z42-test `.z42` 作 seed（Z42_HOME）驱动 z42c 自建。
