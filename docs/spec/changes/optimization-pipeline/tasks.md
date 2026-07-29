# Tasks: 编译期 IR 优化管线

> 状态：🟡 进行中 | 创建：2026-07-30 | 类型：ir（改 z42c emit）

## 进度概览
- [x] 阶段 1: 探查 + 方向裁决（甲=编译器端）+ 准则固化
- [x] 阶段 2: 处理流程框架 + temp-DCE 首发 pass
- [ ] 阶段 3: 验证全绿 + 归档
- [ ] 阶段 4（独立后续）: copy-prop / const-fold 新 pass

## 阶段 1: 探查 + 方向
- [x] 1.1 探查 z42c codegen/元数据产出 + 运行时分析（两 Explore agent）
- [x] 1.2 两准则固化到 book `optimization-pipeline.md`（interp-first / 内存时间开销）
- [x] 1.3 甲/乙 bootstrap 税摆清 → User 裁决甲（编译期）

## 阶段 2: 处理流程 + temp-DCE
- [x] 2.1 `IrOptInfo.z42`：DstId / AddReads / AddTermReads / IsPure（镜像 _regtInstr 保完整；白名单）
- [x] 2.2 `IrOptPipeline.z42`：Run 遍历 + 读计数 + temp-DCE
- [x] 2.3 挂载 IrGen.Generate 末尾
- [x] 2.4 修 out_var 回归：参数寄存器 live-out（正确性不变量，已记 design）

## 阶段 3: 验证
- [x] 3.1 build compiler —— z42c 自建通过（pass 全 stdlib+z42c 源码跑通不崩）
- [x] 3.2 xtask test e2e —— 424/0（interp+jit），out_var OK
- [x] 3.3 xtask test e2e --dir cross-zpkg —— 8/0
- [x] 3.4 xtask test compiler —— 自举不动点 5/5 gen1==gen2 + 20 units
- [ ] 3.5 xtask test stdlib —— [Test] dogfood 全绿（运行中）
- [ ] 3.6 xtask test vscode-syntax —— grammar 一致（应不受影响）
- [ ] 3.7 文档同步：z42.ir/README 或 semantics/README 功能索引加 IrOpt；book 补机制节
- [ ] 3.8 归档 + commit（.claude/ + docs/spec/ 纳入）

## 阶段 4: 后续 pass（独立 change 或续作）
- [ ] 4.1 copy-prop：消 SSA-lite 拷回冗余 Copy（interp 热路大头）+ interp 前后基准
- [ ] 4.2 const-fold：折叠常量 temp 链
- [ ] 4.3 评估迭代到不动点 / MaxReg 下调重编号

## 备注
- 正确性不变量：一个寄存器值 escape 函数的途径 = 返回 / out·ref 参数 / 有副作用指令读；三者齐全 DCE 才安全（见 design.md）。
- temp-DCE 单独收益预期marginal（真实代码少有全死纯值）；真正 interp 杠杆是 copy-prop（阶段 4.1）。本阶段核心交付是**可扩展的处理流程**。
