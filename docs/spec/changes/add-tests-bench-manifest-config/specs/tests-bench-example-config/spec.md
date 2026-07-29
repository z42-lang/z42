# Spec: 清单声明式 test / bench / example 目标

## ADDED Requirements

### Requirement: 段配置（约定扫描 + dev-deps）

#### Scenario: 段全省 → 走默认约定
- **WHEN** `z42.toml` 无 `[tests]` 段
- **THEN** ManifestLoader 返回默认 `TargetSection`：`include=["tests/*.z42","tests/*/source.z42"]`、`exclude=[]`、`auto=true`、`deps=[]`

#### Scenario: 段声明 include/exclude + dev-deps
- **WHEN** `[tests] include=["it/**/*.z42"]` 且 `[tests.dependencies] "z42.test"="0.1.0"`
- **THEN** section.Include=["it/**/*.z42"]、section.Deps 含 z42.test

#### Scenario: auto=false 关闭约定扫描
- **WHEN** `[tests] auto=false`
- **THEN** section.Auto=false；xtask 只发现 `[[test]]` 显式目标，不扫 include glob

### Requirement: 显式目标 `[[test]]`/`[[bench]]`/`[[example]]`

#### Scenario: harness=true（默认，反射）
- **WHEN** `[[test]] name="unit_math"`（无 harness 键）
- **THEN** RunTarget.Name="unit_math"、Harness=true、HasEntry=false；运行时走 z42b 反射 `[Test]`

#### Scenario: harness=false + entry（自带 Main，退出码）
- **WHEN** `[[test]] name="perf" harness=false entry="Perf.Runner.Main" sources=["tests/perf/*.z42"]`
- **THEN** RunTarget.Harness=false、Entry="Perf.Runner.Main"、Sources=["tests/perf/*.z42"]；运行时 `z42vm <artifact> Perf.Runner.Main`，退出码非零即失败

#### Scenario: sources 省略 → 沿用约定单元文件
- **WHEN** `[[test]] name="x" harness=false entry="X.Main"`（无 sources）
- **THEN** RunTarget.SrcCount=0；xtask 用该 name 对应的约定布局文件集编译

#### Scenario: per-target dev-dep 三层合并
- **WHEN** `[dependencies] a`；`[tests.dependencies] b`；`[[test]] name="t"` 下 `[test.dependencies] c`
- **THEN** 目标 t 编译依赖 = {a, b, c}；同名冲突时 target > section > project

### Requirement: example 目标 —— 默认只编不跑

#### Scenario: xtask test 编译所有 example 作门禁
- **WHEN** 工程有 example 目标，运行 `xtask test`
- **THEN** 所有 example 被**编译**（编不过则该 stage 失败），但**不执行**

#### Scenario: test=true 的 example 纳入 xtask test 执行
- **WHEN** `[[example]] name="e" test=true`，运行 `xtask test`
- **THEN** example e 被编译**且执行**，退出码判定

#### Scenario: xtask example <name> 显式跑单个
- **WHEN** `xtask example hello`
- **THEN** 只编译并运行名为 hello 的 example，退出码判定

### Requirement: 具名选择运行

> **CLI 形态（实施定稿）**：具名选择用 `xtask test targets <name>` / `xtask bench targets <name>` /
> `xtask example <name>`——裸 `test`/`bench` 已是**全量 gate / e2e 默认动作**，不能重载为具名选择，
> 故 test/bench 走 `targets <name>` 子动作；`example <name>` 如原设计。

#### Scenario: xtask test targets <name> 只跑一个
- **WHEN** 工程有 test 目标 a/b/c，运行 `xtask test targets b`
- **THEN** 只发现/编译/运行 b

#### Scenario: xtask bench targets <name> 只跑一个
- **WHEN** `xtask bench targets throughput`
- **THEN** 只跑名为 throughput 的 bench 目标

#### Scenario: 名不存在 → 明确报错
- **WHEN** `xtask test targets nonexistent`（无此目标）
- **THEN** 报错列出可用目标名，非零退出（不静默成功）

### Requirement: 约定发现确定性

#### Scenario: 扫描按稳定键排序
- **WHEN** include glob 匹配多个文件/目录，在不同 OS / FS 上运行
- **THEN** auto 目标发现顺序一致（扫描前显式 sort，不依赖 FS 枚举序）

#### Scenario: 显式目标覆盖同名 auto
- **WHEN** 约定扫到 `tests/perf.z42`（auto 名 perf）且存在 `[[test]] name="perf" harness=false`
- **THEN** 以显式 `[[test]]` 为准（harness=false），不重复注册

### Requirement: release 忽略 test/bench/example

#### Scenario: 非 test 构建路径不含 dev 目标
- **WHEN** `xtask build`（release 路径）
- **THEN** 忽略全部 `[tests]`/`[bench]`/`[example]`/`[[test]]`/`[[bench]]`/`[[example]]`；release zpkg 元数据只含 `[dependencies]`

## MODIFIED Requirements

### Requirement: L5b 旧设计取代
**Before（L5b 2026-06-06 纸面）：** `[[test]].src` 单入口文件（WS041 必填）；无 harness；无 example；dir-mode Main + golden 比对隐含。
**After：** `[[test]].entry`（FQ 函数）+ `sources[]`（glob，对齐 `[[exe]]`）；`harness` 布尔；example 一等；harness=false 退出码判定（无 golden）。

## Known Limitations（实施定稿 2026-07-29）

### 自定义段 `include` glob 运行期暂不展开（仅约定 dir）
- **现状**：P1 解析层完整读取 `[tests] include=[...]` 等自定义 glob（单测覆盖）；但 xtask 发现层
  （`_autoUnits`）目前只扫**约定目录** `tests/`·`bench/`·`examples/`，不展开约定目录外的自定义
  `include` glob。
- **影响**：约定布局（绝大多数场景）+ 显式 `[[target]]` 全部正常；仅"用自定义 include 指向非约定
  目录做批量扫描"这一种用法运行期不生效。
- **判据**：当前无任何 stdlib lib 使用自定义 include；无 spec 场景要求运行期展开自定义 glob。
- **后续**：需要时在 `_autoUnits` 接入 `SourceDiscovery` 展开 `section.Include`（标 `LIMITATION`
  注释于源码）。登记见 roadmap Deferred（若升级为正式 backlog）。

## Pipeline Steps
受影响（非语言 pipeline，是工程模型 + 工具链）：
- [x] ManifestLoader 解析（stdlib）
- [x] ProjectManifest 模型（stdlib）
- [x] xtask 发现 / 编译编排 / 运行 / 过滤（toolchain）
- [ ] Lexer / Parser / TypeChecker / IR / VM interp —— **不涉及**（无新语法/IR/VM 语义）
