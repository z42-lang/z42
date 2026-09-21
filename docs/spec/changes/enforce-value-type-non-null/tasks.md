# Tasks: 值类型永不可空

> 状态：🔴 待 User 审批（阶段 6.5 gate 未过，**不得开始写代码**） | 创建：2026-09-21
> 分支/worktree：`ref-null-model` @ `/Users/d.s.qiu/Documents/z42-lang/wt-refnull`（基于 origin/main a50f7e897 #723）
> 类型：`lang` + `vm` —— 完整流程（阶段 1–9）
> **依赖**：`simplify-ref-parameters` 先落地（TryParse 迁移用 `ref` 出参）

## 进度概览
- [ ] 阶段 0: User 审批 proposal + spec + design
- [ ] 阶段 1: **摸底诊断**（Q1，必须最先做）
- [ ] 阶段 2: 编译期 —— 值类型拒绝 null
- [ ] 阶段 3: 运行期 —— 存储零初始化
- [ ] 阶段 4: 运行期 —— 拆箱两段检查
- [ ] 阶段 5: stdlib 迁移
- [ ] 阶段 6: 测试迁移 + 自举 + GREEN
- [ ] 阶段 7: 文档同步 + 归档

---

## 阶段 0: 审批
- [ ] 0.1 User 审批 proposal.md
- [ ] 0.2 User 审批 specs/value-type-nullability/spec.md + design.md
- [ ] 0.3 确认 `simplify-ref-parameters` 已落地（否则 TryParse 改用元组，spec §stdlib 需改写）
- [ ] 0.4 User 明确「可以开始」→ 阶段 6.5 gate 通过

## 阶段 1: 摸底诊断（Q1 —— 本变更唯一的未知数）
> **必须在阶段 3（零初始化）之前完成。** 零初始化会把「用 `f == null` 检测值类型字段没设过」
> 的代码静默改掉（Null → 0，判断恒假，不报错）。grep 查不出来（按名字匹配全是同名引用字段误命中）。
- [ ] 1.1 只加 D6 诊断（值类型与 null 比较 → 报错），**不改运行期**
- [ ] 1.2 全仓构建，收集全部命中
- [ ] 1.3 命中数为 0 ⇒ 记录结论，推进；有命中 ⇒ 逐个判读「检测未设置」vs「冗余检查」，在此列出处理方案
- [ ] 1.4 把命中数与结论写回 proposal Q1

## 阶段 2: 编译期
- [ ] 2.1 `DiagnosticCodes.z42` —— `NullToValueType` / `ValueTypeNullComparison` / `NullableValueTypeNotSupported`
- [ ] 2.2 `SymbolTable.z42:590` —— `NullableType` 值类型分支报 `NullableValueTypeNotSupported`（带迁移提示）；引用类型分支保持擦除
- [ ] 2.3 `AssignTyper` / `ExprTyper` —— null 赋给/传给值类型 → `NullToValueType`（赋值、实参、字段初始化器三条路径）
- [ ] 2.4 D6 诊断收口：未约束泛型 `T` 不报；`object` 不报
- [ ] 2.5 `TypeNameResolver` —— 值类型签名不再拼 `?`
- [ ] 2.6 语义单测覆盖 spec 全部 ADDED 场景

## 阶段 3: 运行期 —— 存储零初始化
> 6 处站点，**解释器与 JIT 必须同一步改完**，否则 tier-up 前后行为分叉。
- [ ] 3.1 `corelib/assemblyloadcontext.rs:37` —— 实例字段按 tag 取 `default_value_for_tag`
- [ ] 3.2 `corelib/diagnostics.rs:38`
- [ ] 3.3 `corelib/reflection/type_object.rs:281` + `:359`
- [ ] 3.4 `interp/struct_arena.rs:84` —— struct 的 ref 槽（值槽走 `bytes` 已正确，确认无需改）
- [ ] 3.5 `jit/frame.rs:138` —— **与 3.1 绑同一提交**
- [ ] 3.6 已有的读取侧补丁（`symres.rs` / `exec_array.rs`）保留，注释标注「根因已在分配点修复，此处兜底」
- [ ] 3.7 Rust 单测：四类值类型字段（int/bool/char/double）+ struct ref 槽 + JIT 路径

## 阶段 4: 运行期 —— 拆箱两段检查
- [ ] 4.1 解释器拆箱路径：① null → `NullReferenceException`（带源位置）② 类型不符 → `InvalidCastException`
- [ ] 4.2 `jit/helpers/*.rs` 同步
- [ ] 4.3 消息文案确认可分辨（两条不同异常、两条不同消息）
- [ ] 4.4 在 `docs/spec/archive/2026-09-18-fix-box-null-nullable/` 留交叉引用：本变更反转其方向，理由见 design §D3

## 阶段 5: stdlib 迁移
- [ ] 5.1 `Primitives/Int32.z42:23` —— `int? TryParse` → `bool TryParse(string, ref int)`
- [ ] 5.2 `Primitives/Int64.z42:20` 同上
- [ ] 5.3 `Primitives/Double.z42:21` 同上
- [ ] 5.4 `Guid.z42:70` —— `Guid?` → `bool TryParse(string, ref Guid)`（`Guid` 是 struct）
- [ ] 5.5 `Version.z42:83` `_parseComponent` —— 唯一调用点改写，对外 `FormatException` 行为不变
- [ ] 5.6 确认 `IPAddress.TryParse` / `ProcessHandle.TryWait` 不动（引用类型，规约的正面样本）

## 阶段 6: 测试迁移 + 自举 + GREEN
- [ ] 6.1 `src/tests/types/nullable_value_types.z42` —— 整个文件前提消失，改写为阴性用例
- [ ] 6.2 `src/tests/types/box_null_nullable.z42` —— 同上 + 拆箱两段检查的正面用例
- [ ] 6.3 `z42.core/tests/scalar_tryparse_classify.z42` —— 改 `ref` 写法
- [ ] 6.4 `z42.core/tests/op_edge_cases.z42:61` —— `bool?` 相关用例
- [ ] 6.5 全仓 grep 清零：值类型 `?` 标注
- [ ] 6.6 按 `bootstrap-seed.md` 走冷种子（stdlib 签名变了）
- [ ] 6.7 `xtask build` + `xtask test all` 全绿；`cargo test -p z42 --lib`（**debug，不能 `--release`**）
- [ ] 6.8 golden 应**不动**（零初始化在运行期，不产生 IR 指令）；若动了 ⇒ 停下查
- [ ] 6.9 确认无 zbc/zpkg 格式 bump
- [ ] 6.10 推 PR 过 CI（覆盖 JIT 路径：`jit-fixpoint`）

## 阶段 7: 文档 + 归档
- [ ] 7.1 `docs/reference/src/language/types.md` —— 值类型永不可空；`T?` 仅引用类型
- [ ] 7.2 同上 —— 新增「可能没有」API 规约（引用类型走 `V?`；值类型走 `bool TryX(ref T)`）
- [ ] 7.3 `docs/roadmap.md` —— 修正「可空标注」与其它十项塞一行整行标 ✅ 的谎报；为剩余的引用类型标记线单独排期
- [ ] 7.4 `docs/internals/` —— 存储初始化与拆箱路径
- [ ] 7.5 归档到 `docs/spec/archive/YYYY-MM-DD-enforce-value-type-non-null/`

---

## 已知未堵的洞（单列 follow-up，不在本变更）
- **泛型 struct 数组的 Null 槽** —— `interp/exec_array.rs` 的补丁注释自述
  「Deliberately narrow: only PRIMITIVE value params」，struct 类型参数仍走老路。
  强推 struct backing 会打坏泛型容器（`struct_generic_container: VCall: expected object, got StructRefHeap`）。

## 后续 change
- `define-null-check-marks` —— 引用类型 `?` 标记 + 流分析 + 反向推导 + `Expect("理由")` + **砍 `??`（生产 69 处）与 `?.`（8 处）**
