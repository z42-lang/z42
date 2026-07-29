# Design: 清单声明式 test / bench / example 目标

> 参照 Cargo target 模型（`[[test]]`/`[[bench]]`/`[[example]]` + auto-discovery），但按 z42
> 自举子集精简。本设计**取代** project.md L5b 的旧纸面设计（2026-06-06）。

## Architecture

```
z42.toml  ──ManifestLoader(stdlib)──►  ProjectManifest
   [tests]/[benches]/[examples] 段      ├─ TargetSection Tests/Bench/Example  (include/exclude/auto + dev-deps)
   [[test]]/[[bench]]/[[example]]       └─ RunTarget[]  Tests/Benches/Examples (name/harness/entry/sources/deps)
                                                 │
                     xtask(toolchain) ◄──────────┘  读段+目标
                        │  1. 约定扫描 include glob → auto RunTarget（确定序 sort）
                        │  2. 合并显式 [[target]]（同名覆盖 auto）
                        │  3. 三层 dep 合并（target > section > [dependencies]）
                        │  4. 每目标：合成 mini-manifest → z42c build（子进程，driver 不改）
                        ▼
              harness=true  → z42vm z42.builder.zpkg -- test <artifact>   (z42b 反射跑 [Test]/[Benchmark])
              harness=false → z42vm <artifact> <entry>                    (跑 Main，退出码判定)
```

**两层模型**（解决"批量 vs 逐个"）：段 = 约定扫描（零配置批量）；`[[target]]` = 显式具名（多文件合并 / 自定义 entry / 独享 dep / harness 覆盖）。二者共存，同名时显式覆盖 auto。

## Decisions

### D1: 目标字段对齐 `[[exe]]`，不沿用 L5b 的 `src`
**问题：** L5b 旧设计 `[[test]].src = "tests/perf/runner.z42"`（单入口**文件**，WS041 必填）+ `sources`（glob）。但 `[[exe]]` 已实现的约定是 `entry`（FQ **函数**名）+ `src[]`（glob）。两者语义冲突（`src` 一处指文件、一处指 glob）。
**决定：** 统一到 `[[exe]]` 范式——`entry`（FQ 函数名，harness=false 时用）+ `sources[]`（编译源集 glob，省略=约定单元的文件）。**理由：** `[[exe]]` 是已落地、已测的实现；一致性 > 复用废弃纸面设计。L5b 重写。

### D2: `harness` 布尔表达"谁驱动"（取代 L5b 无此维度）
**问题：** 现状 xtask 靠 grep 源码有无 `[Test]` 区分"反射跑 vs Main golden"，脆弱。
**决定：** 借 Cargo `harness`——`harness=true`（默认）→ z42b 反射跑 `[Test]`/`[Benchmark]`；`harness=false` → 自带 `entry` Main，直接跑。意图显式化，不再 grep。

### D3: `harness=false` 一律 exit-code 判定，**不引入 expected 字段**
**问题：** z42 有 golden（stdout vs expected_output.txt）能力，harness=false 是否复用？
**决定（User 定，2026-07-28）：** 只看退出码，非零即失败，**不加 expected 字段、不做 stdout 比对**。**理由：** 最简；Main 自断言即可。现有 golden 语料继续作**独立约定 harness**（xtask_test_vm）存在，与本模型正交并存——本 change 不动它，不迁移它。

### D4: example 一等目标，默认"只编不跑"
**问题：** example 无 runner、无 expected；语义空白。
**决定（借 Cargo）：** `xtask test` **编译**所有 example 当门禁（确保永远编得过），**默认不执行**；`xtask example <name>` 显式跑（exit-code）；目标写 `test = true` 则纳入 `xtask test` 执行。example 恒 Main 程序，`harness` 对其无意义（模型里 example 目标忽略 harness，走 entry Main）。

### D5: 约定发现 = include glob + 路径推导名 + 确定序
**决定：** 段 `include` 默认值——`[tests]`→`tests/*.z42`+`tests/*/source.z42`；`[benches]`→`bench/*.z42`+`bench/*/source.z42`；`[examples]`→`examples/*.z42`+`examples/*/source.z42`。auto 名：文件 stem / 目录名。`auto=false` 关闭扫描、只认 `[[target]]`。**扫描循环必须先按稳定键 sort**（[common-pitfalls §1](../../../../.claude/rules/common-pitfalls.md)——first-wins 注册禁止依赖 FS 枚举序）。（段名复数、发现 dir 仍用既有约定 `tests/`·`bench/`·`examples/`——段名与 dir 名无需一致。）

### D6: 三层 dep 合并（复用 L5b，唯一保留的旧设计）
```
final_deps = [dependencies] ∪ [<kind>.dependencies] ∪ [[target]].dependencies
优先级：[[target]] > [<kind>] > [dependencies]（精确覆盖广泛）
```
`[<kind>.dependencies]`（如 `[tests.dependencies]`）仅测试/bench/example 编译时合入，release zpkg 元数据不含（release 忽略全部 test/bench/example 段）。

### D7: 模型用单一 `RunTarget` 类，三个数组
**问题：** test/bench/example 结构同构（name/harness/entry/sources/deps）；z42 自举子集无泛型。
**决定：** 一个 `RunTarget` 类复用三处；`ProjectManifest` 存 `Tests[]`/`Benches[]`/`Examples[]` + 三个 count。段用一个 `TargetSection` 类复用三处。**理由：** DRY，符合现有 `ExeTarget` 单类风格；避免三套近似类。

### D8: 段名复数、目标数组单数（修 L5b 的 TOML key 冲突 bug）
**问题：** L5b 旧设计用 `[bench]` 段 + `[[bench]]` 数组（`example` 同理）——**同一 TOML key 既是 table 又是 array-of-tables 是非法 TOML**，Std.Toml 会报错。`[tests]`/`[[test]]` 因单复数不同侥幸不撞。
**决定：** **段名一律复数、目标数组一律单数**——`[tests]`/`[benches]`/`[examples]` 段 + `[[test]]`/`[[bench]]`/`[[example]]` 目标，三对 key 全不撞；dep 子表 `[<plural>.dependencies]`。发现 dir 仍沿用既有约定 `tests/`·`bench/`·`examples/`（段名≠dir 名，无碍）。**理由：** TOML 有效性硬约束；复数=段/单数=目标是清晰助记，与已有 `[tests]` 一致。（实施期发现，2026-07-29。）

## Implementation Notes

- **z42.project 自举子集写法**：sealed class + 构造函数、`bool HasX` 替 nullable、`array + count` 替泛型、无 record/泛型。`RunTarget`/`TargetSection` 照此。
- **解析器**：`_parseTargetSection(root, "tests")`（段缺失→默认 include + auto=true + 空 deps）；`_parseRunTargets(root, "test")`（读 `root.Get("test")` array-of-tables，仿 `_parseExes`）。三 kind 各调一次。
- **错误码**（更新 L5b）：`[[target]]` 缺 `name` → error；同 kind 内 name 重复 → error；`harness=false` 缺 `entry` → error；auto 名与显式目标名冲突 → 显式覆盖（非错误，可 warn）。
- **xtask 发现顺序**：扫 include glob → 生成 auto RunTarget（sort）→ 用显式 `[[target]]` 按 name 覆盖/追加 → filter（`<name>` 参数）→ 编译 + 运行。
- **harness 分派**：复用 [xtask_test_lib_units.z42](../../../../scripts/test/xtask_test_lib_units.z42) 的 `_runUnitsBatched`（harness=true 路径已存在）；新增 harness=false 分支直接 `z42vm <artifact> <entry>` 判退出码。

## Testing Strategy
- **单元（解析）**：`z42.project/tests/tests_bench_example_targets.z42`——段默认值 / include-exclude / auto 开关 / `[[target]]` 字段 / harness 默认 / dev-deps 三层 / 缺 name / harness=false 缺 entry / 重名。
- **端到端**：一个含 `[[test]]`(harness=true)+`[[test]]`(harness=false)+`[[bench]]`+`[[example]]`(test=true) 的工程夹具 → `xtask test <name>` 选中单个、`xtask example <name>` 跑单个、退出码判定、约定 glob 兜底。
- **GREEN**：`xtask test` 全 stage（含新 example 编译门禁）+ stdlib z42.project 单测 + 自举不动点（z42.project 改动不得漂移 z42c 字节——它是 stdlib，由自建 z42c 编，A/B 验证）。CI 权威（cold worktree 本地不可验自举链）。

## 实施定稿差异（2026-07-29，落地时确定）

1. **CLI 形态：`xtask test targets <name>` / `bench targets <name>` / `example <name>`**。裸 `test`/`bench`
   已是全量 gate / e2e 默认动作，无法重载为具名选择，故 test/bench 走 `targets <name>` 子动作；`example`
   如原设计。spec「具名选择运行」已同步。
2. **xtask 不加 `[dependencies]` 段消费 z42.project**（事实校正）。z42c 的 `DepScan`（`DepScan.z42:126`）：
   工程声明 0 依赖时**索引全部 `Z42_LIBS`**；加一个部分 `[dependencies]` 块会翻成「仅声明可见」→ 破坏
   `Std.Cli`/`Std.IO`/`Z42.Build` 解析（precedent：`scripts/hooks/hooks.z42` 用 `using Z42.Build;` 且无 deps 块）。
   故 xtask 直接 `using Z42.Build.Project;`，z42.project.zpkg 从 flat alllibs 目录解析，**不改 xtask.z42.toml**。
3. **harness=false 运行路径**：合成 `kind=exe` mini-manifest（`[project].entry` 烤入 target.Entry）→ `z42c build`
   → `z42vm <zpkg>`（跑烤入 entry，不传 CLI entry 参数，对齐 multi-exe 已验证路径），退出码判定。
4. **自定义段 `include` glob 运行期暂不展开**（仅约定 dir `tests/`·`bench/`·`examples/`）——见 spec「Known
   Limitations」。解析层已支持，发现层后续接入 `SourceDiscovery`。
