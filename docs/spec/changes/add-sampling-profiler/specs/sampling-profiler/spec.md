# Spec: sampling-profiler（safepoint 采样火焰图）

## ADDED Requirements

### Requirement: 采样默认关，零成本

#### Scenario: Z42_SAMPLE_HZ 未设 → 无采样、无后台线程
- **WHEN** 不设 `Z42_SAMPLE_HZ` 运行任意脚本
- **THEN** 不启动采样后台线程；`sample_pending` 永不置；`check_safepoint_slow` 只多一次 atomic load
  （~1/throttle_n 频率，可忽略）；不产出 folded 文件

#### Scenario: 采样 disabled 时热路径不受影响
- **WHEN** 采样关
- **THEN** `check_safepoint` 快路径完全不含采样代码；采样检查只在已经 throttle 到的 slow path、且 gated 在
  `sampler.enabled` 一次 load 后

### Requirement: 采样捕获 z42 调用栈并聚合

#### Scenario: 采样命中记录当前 z42 栈
- **WHEN** `Z42_SAMPLE_HZ=1000`，脚本在一个热循环里调 `foo() → bar()`，采样线程置 flag
- **THEN** 下一个到达 safepoint 的 mutator 快照 `call_stack` → folded stack（如 `Main;foo;bar`）→ 累加器
  该键计数 +1；多次采样后热路径的键计数最高

#### Scenario: folded stack 格式可被 flamegraph 消费
- **WHEN** 运行结束
- **THEN** 输出文件每行 `frame1;frame2;…;frameN <count>`（分号分隔、栈底在左、空格 + 计数结尾），
  即 inferno / flamegraph.pl 标准输入；`Z42_SAMPLE_OUT` 指定路径（默认 `z42-samples.folded`）

#### Scenario: 空栈 / 无采样命中不产出坏行
- **WHEN** 采样开但程序太短没被采到
- **THEN** 输出文件为空或不产出（不写 `<空> 0` 之类坏行）；不 panic

### Requirement: xtask profile 产 z42 火焰图

#### Scenario: xtask profile --cpu 增 z42-level 火焰图
- **WHEN** `xtask profile --cpu <script>`
- **THEN** 除 samply（native）外，用 `Z42_SAMPLE_HZ` 跑一遍收 folded stacks；有 `inferno` CLI 则渲 SVG
  火焰图，否则落 `.folded` 文件 + 查看提示（镜像 `--heap` dhat 产物模式）；无论哪种都不让 profile 失败

## IR Mapping

无新 IR 指令 / 无 zbc·zpkg 格式变更（纯运行时采样 + 文本输出）。

## Pipeline Steps

- [x] VM interp / runtime —— sampler.rs + safepoint hook + VmCore 字段 + app.rs flush
- [x] config —— Z42_SAMPLE_HZ / Z42_SAMPLE_OUT knobs
- [x] toolchain —— xtask profile z42-level 火焰图
- [x] docs —— book 诊断机制页（知识上浮）
- [ ] Lexer/Parser/TypeChecker/Codegen —— 无
