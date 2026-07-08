# Tasks: 文件级增量编译（cache SoT → dist 投影）

> 状态：🟢 已完成 | 创建：2026-07-07 | 完成：2026-07-08
> 占用子系统：`compiler`（ACTIVE.md 已登记）
> 变更类型：feat(compiler)；零 wire 格式 bump、VM 零改动
> 前继：port-incremental-build-cache（2026-07-05 归档）；后继：add-indexed-zpkg-min-patch（DRAFT）

## 进度概览
- [x] 阶段 1: 调查 + CacheStore（meta 格式与读写）
- [x] 阶段 2: ZbcReader 移植 + 往返单测
- [x] 阶段 3: 依赖图 + 失效闭包（IncrementalBuild 升级）
- [x] 阶段 4: cached 重建 + dist 从 cache 组装（driver/semantics 集成；e2e 四场景字节实证）
- [x] 阶段 5: 暴力对账器 + 全量验证 + 文档 + 归档（含 D8 三轮性能修正）

## 阶段 1: 调查 + CacheStore
- [x] 1.1 调查完成（2026-07-08，User 确认）：每文件 TSIG 全包耦合（allFuncs 兄弟泄漏 + 全包 AST 依赖）→ **不做 cachedExports 注入**，改为全包 parse+TSIG 重算、仅失效子集 typecheck/codegen；D2 已重写、proposal What #1/#3 已收窄
- [x] 1.2 `CacheStore.z42`：meta 序列化（hash/ns/usedDepNs/引用定义文件集/版本 pin；**无 TSIG**）+ Load/Save + 版本失效（阶段 4 增 pool/labels 残留字段，见 D5a）
- [x] 1.3 CacheStore 单测 ×3（往返 / zbc 成对约束 / pin 失效+未知键+空 hash → null）

## 阶段 2: ZbcReader
- [x] 2.1 移植完成：`ZbcReader.z42`（cursor/目录/NSPC/STRS/TYPE/SIGS/FUNC/REGT/DBUG/TIDX + 模块池重建）+ `ZbcReaderInstr.z42`（指令/终结符解码 + REGT 类型回填 Retype——文件拆分为守 500 行硬限，Scope 备注增补）；IMPT/EXPT 不解码（writer 重推导）；未知 opcode/截断/pin 不符 → null 降级 fresh
- [x] 2.2 `z42c.ir/tests/zbcreader/` 往返单测 ×6 全过（算术+局部调用 token / 字面量含 f64 bits / 类+字段+静态 / 数组四操作 / 负字面量 hex 重表示 / 坏 magic+minor 拒绝）；自举不动点 7/7 不回归
- [x] 2.3（实施期发现，D5a）wire 信息差实证：块 label 语义命名 + 模块池原序 + TIDX idx = writer 残留 → 存 meta、集成层回填；roundtrip 语料限单块函数，多块路径由暴力对账器验收

## 阶段 3: 依赖图 + 失效
- [x] 3.1 失效边实现修订（D3 落地形态）：token 保守边——文件 i 标识符 token ∩ 文件 j 包内定义名（类型+自由函数+成员名，成员名保证推断透传完备）→ 边；每次 build 从当前源重算，**不入 meta**（比原案 typechecker 插桩更简单且严格超集）
- [x] 3.2 `ProbeFiles`（种子：hash/条目缺失·pin/包级源清单不一致→全量）+ `Close`（StrMap 属主索引 + 闭包迭代）→ IncrFilePlan；旧整包 Probe 删除
- [x] 3.3 `Z42_INCR_DEBUG` 种子原因 + 传播链（`b.z42 invalidated-by a.z42` e2e 实证）；单测 6 个（种子四态 + 闭包链式/无边）

## 阶段 4: 集成
- [x] 4.1 cachedSet 重建：`IncrementalDriver.Prepare`（读 zbc → ZbcReader → meta 残留回填 `_applyResidue`：label 改名（块/终结符/异常表）+ 模块池原序 + TIDX idx；失败降级 fresh + 重闭包）
- [x] 4.2 D2 修正形态落地：`IrDump.ParseAll` + `BuildPackageCus(cachedModules)`——cached 跳过 `_compileCu`，TSIG 全包重算；fresh 落 cache（zbc+meta+包级清单，`WriteMetas`）；cached 的 UsedDepNs 从 meta 回填
- [x] 4.3 dist 组装输入 = cached IrModule + fresh IrModule 同构进 `BuildPackedD`（D7 零分叉）；全未失效仍走 preserved 跳过路径（e2e ✓）
- [x] 4.4 `ReadSourceHashes` 保留为 zpkg wire 读取工具（MODS 头仍携带源 hash + 单测），probe 消费退役；旧 `IncrProbeResult`/整包 Probe 删除
- [x] 4.5（实施发现并修复）ZbcCursor.U32 组装值在 z42 int 移位语义下 bit31 不为负：import token 判定 `<0` 恒假 → 'P.A' 解析成 ""（e2e 字节漂移 +8 逮出）；u32 0xFFFFFFFF 哨兵 `!= (0-1)` 同坑（TYPE base / catchType / DBUG file 三处）→ 显式 bit31 检测 `_isImport`/`_isNoneU32`
- [x] 4.6 e2e 四场景字节实证（受控三文件工程）：叶子 touch → cached 2/3 **byte-identical**；被依赖 touch → 闭包失效 b、cached 1/3 **byte-identical**；增文件 → 全量 0/4；no-touch → preserved

## 阶段 5: 验收 + 文档
- [x] 5.0（实施期发现，D8 回退信号三轮修正，User 追认 Parser.z42 Scope 2026-07-08）：
  首轮对账器 xtask 语料增量比全量**慢 17%** → ① token 集入 meta v2（cached 行零重 lex）；
  ② `Parser.Tokens()` 访问器 + `ParseAllTk` parse 时捕获标识符（fresh 行零第二遍 lex；
  `--no-incremental`/全量路径同享）；③ 源 SHA-256 从 3 遍收敛为 1 遍（Main 算一次，
  probe/ZbcFileZ/meta 复用）。终态 worst-case 语料 **+1.5% ≈ 持平**、全量提速 8s、
  no-touch 9×、字节仍一致（design D8 实测记录）
- [x] 5.1 暴力对账器 `xtask test incremental` + CLI 注册 + 计时报告。语料修订（备注）：z42c 7 包走 --workspace 不落 cache → 语料 = demo 合成多特性工程 + xtask 42 文件真实工程。**两轮全扫**：修正前（逮出 -17% 倒挂）与最终代码上重跑——**47/47 byte-identical**；终版计时 demo 增量反超（2749 vs 2768ms/touch）、xtask worst-case +1.9%（79.1 vs 77.7s/touch）
- [x] 5.2 场景 e2e：已由 4.6 受控三文件工程字节实证覆盖（叶子/被依赖闭包/增文件全量/no-touch preserved）；真实包语料由 5.1 对账器兜底（demo 多特性 + xtask 42 文件）
- [x] 5.3 `xtask test` 全绿（e2e + stdlib + compiler + vscode-syntax 全 stage，含自举不动点）——在含全部 D8 修正的最终代码上通过
- [x] 5.4 文档：project.md 增量节文件级改写 / book build.md / 4 README / roadmap Deferred（`incremental-future-tsig-level-invalidation`）/ verify-by-change 覆盖矩阵 / scripts README `test incremental`
- [x] 5.5 ACTIVE.md 释锁；归档

## 备注
- 格式前提已核实：z42 泛型 = 代码共享 + 具化（无跨文件单态化拷贝）→ 文件级失效在格式层成立；
  暴力对账器负责实证兜底（proposal Open Question 2）。
- C# 混合重建失败根因 = 两来源合并；本设计单源化（全部来自 cache）在结构上消除该根因。
