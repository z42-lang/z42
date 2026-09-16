# fix-ref-param-escape — tasks

> 类型：fix（最小化模式：只写 tasks.md，见 workflow.md 阶段触发表）
> 状态：🟢 实施完成，GREEN 已跑

## 问题

release 构建下，局部数组以 `ref` 传给被调函数、在被调函数里被重新赋值为新数组后，读回时 VM 报
`stack-alloc array handle used after its creating frame exited`。debug 不复现（debug = `Opt.None`）。

最小复现（`z42c build --release` 后运行）：

```z42
void Fill(ref string[] a) { a = new string[2]; a[0] = "x"; a[1] = "y"; }
void Main() { string[] got = new string[0]; Fill(ref got); Console.WriteLine(Concat("", got)); }
```

来源：归档 change `add-learn-book-examples-gate` 的 tasks.md 备注（2026-09-16 学习手册线发现，Scope 外挂起）。

## 根因

`ref`/`out` 走「入口 copy-in / 出口 copy-out」（`impl-ref-out-in-runtime`）：`exec_function_body` 入口把持
`Value::Ref` 的参数寄存器解引用成底层值，`run_ref_writebacks` 在每条退出路径把该寄存器的**终值**写回
caller 的 lvalue。而 **callee 的 IR 看不见 `ref` 修饰**（`Param.IsRef` 只影响 caller 侧发
`load_local_addr`，callee 寄存器类型不变，见 `syntax/Decl.z42:12`）——于是 `IrEscapeAnalysis` 的汇点规则表里
没有任何一条能表达「写回」，`a = new string[2]` 的全部使用逐条看都是中性的 → 判不逃逸 → 栈分配在 callee
帧 → 写回 caller 的是已退出帧的悬垂句柄。

## 修复

- `IrEscapeAnalysis.ComputeEscapedRegs` 加 **Pass A′**：凡被函数体重新定义过的参数槽一律标逃逸
  （写回的值必是某条 def 的产物）。未被重定义的参数槽不标——终值 = 入口值，本就活得比本帧久。
- 新增 `markRefWriteback` 形参：`_markFunc`（本函数内栈分配判定）传 `true`；`IrEscapeSummary` 的摘要不动点
  传 `false`——摘要问的是「入参值是否逃逸」，参数槽被重新赋值与之无关；一并标会把「循环里推进形参」
  判成参数逃逸，白白让所有调用方的实参丢掉栈分配。
- 精度取舍：IR 层没有 per-param 的 ref 标志，不精确识别；代价是非 ref 形参被重新赋值时也挡掉一次栈分配。

## 顺带补上的门禁缺口（本 change 的另一半）

**这个 bug 能漏出去，是因为逃逸分析在门禁里一次也没被开过。** `--emit-zbc`（golden 用例的编译路径）的默认
优化集减掉了 `StackAlloc` / `Inline` / `LoopAllocReuse` / `PureCall` / `DeadBranch` / `Devirt`（它们会改
golden 字节），而 book 里写的「真实 release 自建 + 专项单测覆盖」中的专项单测**从来没被写出来过**——
`src/tests/optimization/escape_*.z42` 两个用例一直在测「优化关着时」的行为。

- `z42c --emit-zbc <src> <out> [--opt-all]`：按 `Opt.All` 编。
- golden `opt_all` sidecar（dir 模式 `opt_all` / flat 模式 `<name>.opt_all`，与 `interp_only` 同形态），
  `test e2e` 与 `test dist` 两条编译路径都认。
- `src/tests/optimization/` 全体 + `closures/closure_l3_stack` 挂上 `opt_all`；10 个用例在全优化下
  interp/jit 双模式全绿。

## 验证

- [x] 最小复现工程 `z42c build --release` → 修复前崩、修复后输出 `xy`
- [x] **阴性对照（同一套工具链，只换编译器二进制）**：撤回 `IrEscapeAnalysis`/`IrEscapeSummary` 改动重编
      → 复现工程重新崩；新 golden 用例 `escape_ref_param_writeback` interp/jit **双双变红**；恢复后全绿
- [x] 新回归用例 `src/tests/optimization/escape_ref_param_writeback/`（ref 数组 / out 对象 / 两级嵌套写回，
      每步之间插 `Churn` 踩脏帧 arena 防假绿）+ `opt_all` sidecar
- [x] `xtask test e2e --dir optimization`：20 passed, 0 failed
- [x] `xtask test` 全量 GREEN

## 文档

- `docs/internals/src/runtime/escape-analysis.md`：新增「`ref`/`out` 形参的出口写回是逃逸汇点」一节
  + 汇点规则表加一行 + 引擎改三趟 + 页头对齐行
- `src/tests/README.md`：sidecar 表加 `opt_all`、类别表加 `optimization/` 行（含「不带 = 没测」的警告）、
  flat 模式 sidecar 说明
