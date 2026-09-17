# Tasks: workload 命令分发 B1+B2

> 状态：🟡 进行中（可立即开始，无硬前置）| 创建：2026-06-19
> **子系统**：toolchain（launcher）+ stdlib（z42.cli）

## 进度概览

- [ ] 阶段 1: Std.Cli — `AddSpawn` spawn 式叶子
- [ ] 阶段 2: 命令发现（WorkloadCommandDiscovery）
- [ ] 阶段 3: Launcher 集成 + 发现注册
- [ ] 阶段 4: 示例 greet workload
- [ ] 阶段 5: GREEN 验证

---

## 阶段 1: Std.Cli — AddSpawn
> 子系统锁：stdlib

- [ ] 1.1 阅读 `src/libraries/z42.cli/src/SubcommandRouter.z42` 理解现有 dispatch 结构
- [ ] 1.2 新增 `SpawnLeaf` 内部类型（存储 zpkgPath + description）
      — 或直接在 `SubcommandRouter` 里用一个 `spawnEntries: Map<string, SpawnEntry>`
- [ ] 1.3 实现 `SubcommandRouter.AddSpawn(verb: string, desc: string, zpkgPath: string)`
      — 注册到 spawn entries map，不和 ArgParser entries 冲突
- [ ] 1.4 修改 SubcommandRouter 的 dispatch 逻辑：
      — verb 命中 spawn entry → 返回 `CommandResolution.Spawn(zpkgPath, remainingArgv)`
      — `z42 --help` 在列表中显示 spawn 命令及其 desc
- [ ] 1.5 新增 `CommandResolution.Spawn` variant（或复用现有结构）
- [ ] 1.6 单元测试 `[Test]`：`AddSpawn` 注册后 dispatch 返回 Spawn resolution

## 阶段 2: 命令发现（WorkloadCommandDiscovery）
> 子系统锁：toolchain

- [ ] 2.1 新建 `src/toolchain/launcher/core/launcher_workload_commands.z42`
- [ ] 2.2 实现 `.cmd.toml` 解析（读取 verb / description / zpkg 三字段）
      — 使用 `Std.Toml` 或简单行解析（视 Std.Toml key= 支持情况）
- [ ] 2.3 实现 `WorkloadCommandDiscovery.Discover(root: SubcommandRouter, runtimeDir: string)`：
      ```
      workloads_dir = runtimeDir + "/workloads/"
      for each workloadDir in Directory.GetDirectories(workloads_dir).OrderBy(Ordinal):  // ← 必须 sort
          cmds_dir = workloadDir + "/commands/"
          if !Directory.Exists(cmds_dir): continue
          for each cmdToml in Directory.GetFiles(cmds_dir, "*.cmd.toml").OrderBy(Ordinal):
              entry = ParseCmdToml(cmdToml)
              if entry == null: warn + continue
              zpkgAbsPath = Path.Combine(cmds_dir, entry.Zpkg)
              if !File.Exists(zpkgAbsPath): warn + continue
              if root.Has(entry.Verb): continue  // first-wins
              root.AddSpawn(entry.Verb, entry.Description, zpkgAbsPath)
      ```
- [ ] 2.4 单元测试：给定 mock 目录结构，Discover 正确注册命令

## 阶段 3: Launcher 集成 + Spawn 执行
> 子系统锁：toolchain

- [ ] 3.1 修改 `src/toolchain/launcher/core/launcher.z42`：
      — 新增 `_spawnCommand(zpkgPath: string, argv: string[]): int`
        （复用现有 `_runWithVm` 的 Process spawn 逻辑）
- [ ] 3.2 修改 `src/toolchain/launcher/core/launcher_cli.z42`：
      — 在命令注册完毕后调 `WorkloadCommandDiscovery.Discover(root, runtimeDir)`
      — dispatch 结果为 `CommandResolution.Spawn` 时调 `_spawnCommand`

## 阶段 4: 示例 greet workload

- [ ] 4.1 新建 `examples/workloads/greet/greet.z42`（最简：打印 "Hello, world!"）
- [ ] 4.2 新建 `examples/workloads/greet/greet.z42.toml`（项目 manifest）
- [ ] 4.3 新建 `examples/workloads/greet/greet.cmd.toml`
      ```toml
      verb = "greet"
      description = "Say hello (example workload)"
      zpkg = "greet.zpkg"
      ```
- [ ] 4.4 编译验证：`z42c build examples/workloads/greet/greet.z42.toml` 成功
- [ ] 4.5 手动安装测试（文档/脚本）：cp greet.zpkg + greet.cmd.toml 到正确目录
- [ ] 4.6 端到端：`z42 greet` 输出 "Hello, world!"，`z42 -h` 列出 greet

## 阶段 5: GREEN 验证

- [ ] 5.1 `dotnet build` — 无错（stdlib z42.cli 变更）
- [ ] 5.2 `cargo build --release` — 无错
- [ ] 5.3 `dotnet test` — 全绿
- [ ] 5.4 `z42 xtask.zpkg test vm` — 全绿
- [ ] 5.5 `z42 xtask.zpkg test lib` — 22/22（z42.cli 新 [Test] 通过）
- [ ] 5.6 端到端 greet 验证（手动）
- [ ] 5.7 更新 `docs/design/toolchain/launcher-command-dispatch.md`：
          将 B1/B2 从 Deferred 移出，补实施决策摘要
- [ ] 5.8 更新 `docs/internals/src/toolchain/workload-distribution.md`：
          补 `commands/` 目录结构说明
- [ ] 5.9 docs/roadmap.md 0.3.14 退出标准打 ✅

## 备注

- 阶段 1（Std.Cli AddSpawn）和阶段 2（WorkloadCommandDiscovery）子系统不同，可并行进行
- `.cmd.toml` 解析如 Std.Toml 尚未支持，可临时用简单行解析（形如 `key = "value"`）
- 如 `Std.Toml` 已支持，优先复用
