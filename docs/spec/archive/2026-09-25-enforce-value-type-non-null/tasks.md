# Tasks: 值类型永不可空

> 状态：🟢 已完成 | 创建：2026-09-21 | 完成：2026-09-25
> 实现随 **#741** 合入（编译期半部 + stdlib 迁移）；阶段 4「拆箱两段检查」的全部内容
> **由 #746（`make-hard-cast-fail-properly`）实际做掉**，2026-09-25 实跑对账确认（见阶段 4）。
> 分支/worktree：`ref-null-model` @ `/Users/d.s.qiu/Documents/z42-lang/wt-refnull`（基于 origin/main a50f7e897 #723）
> 类型：`lang` + `vm` —— 完整流程（阶段 1–9）
> **依赖**：`simplify-ref-parameters` 先落地（TryParse 迁移用 `ref` 出参）

## 进度概览
- [x] 阶段 0: User 审批 proposal + spec + design
- [x] 阶段 1: **摸底诊断**（Q1，必须最先做）
- [x] 阶段 2: 编译期 —— 值类型拒绝 null
- [x] 阶段 3: ~~运行期 —— 存储零初始化~~ —— **不做**：非泛型字段实测已对（`value_field_zero` 钉住）。
      ⚠️ 2026-09-25 更正：这个结论**当初记宽了**，泛型型参字段那格是洞，见文末「已知未堵的洞」
- [x] 阶段 4: 运行期 —— 拆箱两段检查 —— **由 #746 做掉**，已实跑对账
- [x] 阶段 5: stdlib 迁移
- [x] 阶段 6: 测试迁移 + 自举 + GREEN
- [x] 阶段 7: 文档同步 + 归档

---

## 阶段 0: 审批
- [x] 0.1 User 审批 proposal.md
- [x] 0.2 User 审批 specs/value-type-nullability/spec.md + design.md
- [x] 0.3 确认 `simplify-ref-parameters` 已落地（否则 TryParse 改用元组，spec §stdlib 需改写）
- [x] 0.4 User 明确「可以开始」→ 阶段 6.5 gate 通过

## 阶段 1: 摸底诊断（Q1 —— 本变更唯一的未知数）
> **必须在阶段 3（零初始化）之前完成。** 零初始化会把「用 `f == null` 检测值类型字段没设过」
> 的代码静默改掉（Null → 0，判断恒假，不报错）。grep 查不出来（按名字匹配全是同名引用字段误命中）。
- [x] 1.1 只加 D6 诊断（值类型与 null 比较 → 报错），**不改运行期**
- [x] 1.2 全仓构建，收集全部命中
- [x] 1.3 命中数为 0 ⇒ 记录结论，推进；有命中 ⇒ 逐个判读「检测未设置」vs「冗余检查」，在此列出处理方案
- [x] 1.4 把命中数与结论写回 proposal Q1

## 阶段 2: 编译期
- [x] 2.1 `DiagnosticCodes.z42` —— `NullToValueType` / `ValueTypeNullComparison` / `NullableValueTypeNotSupported`
- [x] 2.2 `SymbolTable.z42:590` —— `NullableType` 值类型分支报 `NullableValueTypeNotSupported`（带迁移提示）；引用类型分支保持擦除
- [x] 2.3 `AssignTyper` / `ExprTyper` —— null 赋给/传给值类型 → `NullToValueType`（赋值、实参、字段初始化器三条路径）
- [x] 2.4 D6 诊断收口：未约束泛型 `T` 不报；`object` 不报
- [x] 2.5 `TypeNameResolver` —— 值类型签名不再拼 `?`
- [x] 2.6 语义单测覆盖 spec 全部 ADDED 场景

## ~~阶段 3: 运行期 —— 存储零初始化~~（实测已经是对的，见 proposal）
> 6 处站点，**解释器与 JIT 必须同一步改完**，否则 tier-up 前后行为分叉。
> **2026-09-25 复核**：整阶段**不做**的结论对**非泛型**成立（`unify-object-byte-layout` 之后
> 对象槽按布局整块零初始化 ⇒ 值类型字段读出零值），但对**泛型型参字段不成立** —— 见文末
> 「已知未堵的洞」新增的那条。⇒ 下面 7 条全部按「不需要改」勾销，洞另立 follow-up。
- [x] ~~3.1 `corelib/assemblyloadcontext.rs:37`~~ —— 不需要：`alloc_object` 走 `object_storage()` 整块零初始化
- [x] ~~3.2 `corelib/diagnostics.rs:38`~~ —— 同上
- [x] ~~3.3 `corelib/reflection/type_object.rs:281` + `:359`~~ —— 同上
- [x] ~~3.4 `interp/struct_arena.rs:84`~~ —— 值槽走 `bytes` 本就正确，ref 槽的 `Null` 是引用类型的零值
- [x] ~~3.5 `jit/frame.rs:138`~~ —— 帧槽的 `Null` 同上
- [x] ~~3.6 读取侧补丁加注释~~ —— 根因不在分配点，注释前提不成立
- [x] ~~3.7 Rust 单测~~ —— 改为 e2e `src/tests/types/value_field_zero/`（随 #741 落地）

## 阶段 4: 运行期 —— 拆箱两段检查 → ~~拆为 follow-up `split-unbox-null-and-type-check`~~
> **2026-09-25 结论：follow-up 不必立，内容已由 #746（`make-hard-cast-fail-properly`）做掉。**
> 两条线在此重叠、此前没人对账。判据不取任何一边的文档，取实跑：
>
> ```
> $ z42vm artifacts/build/tests/types/hard_cast_value/source.zbc Main --mode interp
> PROBE1 NullReferenceException | cannot unbox null to `int`
> PROBE2 InvalidCastException  | cannot cast string to `int`
> $ …--mode jit          # 两行字节相同
> ```
>
> 阴性对照：把用例里 ① 的期望改成 `InvalidCastException`，interp + jit **双双变红**
> ⇒ 用例真的走到过那两条路，不是「恒绿的摆设」。
- [x] 4.1 解释器拆箱路径：① null → `NullReferenceException` ② 类型不符 → `InvalidCastException`
      —— #746 落地，用例 `src/tests/types/hard_cast_value/`
- [x] 4.2 `jit/helpers/*.rs` 同步 —— jit 模式实跑两条消息与 interp 一致
- [x] 4.3 消息文案确认可分辨 —— 两种异常 + 两条消息（`cannot unbox null to …` / `cannot cast … to …`）
- [x] 4.4 在 `docs/spec/archive/2026-09-18-fix-box-null-nullable/` 留交叉引用 —— 本 PR 落地

## 阶段 5: stdlib 迁移
- [x] 5.1 `Primitives/Int32.z42:23` —— `int? TryParse` → `bool TryParse(string, ref int)`
- [x] 5.2 `Primitives/Int64.z42:20` 同上
- [x] 5.3 `Primitives/Double.z42:21` 同上
- [x] 5.4 `Guid.z42:70` —— `Guid?` → `bool TryParse(string, ref Guid)`（`Guid` 是 struct）
- [x] 5.5 `Version.z42:83` `_parseComponent` —— 唯一调用点改写，对外 `FormatException` 行为不变
- [x] 5.6 确认 `IPAddress.TryParse` / `ProcessHandle.TryWait` 不动（引用类型，规约的正面样本）

## 阶段 6: 测试迁移 + 自举 + GREEN
- [x] 6.1 `src/tests/types/nullable_value_types.z42` —— 整个文件前提消失，改写为阴性用例
- [x] 6.2 `src/tests/types/box_null_nullable.z42` —— 同上 + 拆箱两段检查的正面用例
- [x] 6.3 `z42.core/tests/scalar_tryparse_classify.z42` —— 改 `ref` 写法
- [x] 6.4 `z42.core/tests/op_edge_cases.z42:61` —— `bool?` 相关用例
- [x] 6.5 全仓 grep 清零：值类型 `?` 标注
- [x] 6.6 按 `bootstrap-seed.md` 走冷种子（stdlib 签名变了）—— 随 #741 的 CI 完成
- [x] 6.7 `xtask build` + `xtask test all` 全绿；`cargo test -p z42 --lib` —— 随 #741 的 CI 完成
- [x] 6.8 golden 应**不动** —— 成立：#741 的 diff 里 31 个文件**一个 golden 都没有**
- [x] 6.9 确认无 zbc/zpkg 格式 bump —— 成立：#741 的 diff 未动 `CompilerFingerprint` / 格式常量
- [x] 6.10 推 PR 过 CI（覆盖 JIT 路径：`jit-fixpoint`）

## 阶段 7: 文档 + 归档
- [x] 7.1 `docs/reference/src/language/types.md` —— 值类型永不可空；`T?` 仅引用类型
- [x] 7.2 同上 —— 新增「可能没有」API 规约（引用类型走 `V?`；值类型走 `bool TryX(ref T)`）
- [x] 7.3 `docs/roadmap.md` —— 修正「可空标注」与其它十项塞一行整行标 ✅ 的谎报；为剩余的引用类型标记线单独排期
- [x] 7.4 `docs/internals/` —— 存储初始化与拆箱路径（`runtime/object-abi.md` §槽位零初始化，本 PR 落地）
- [x] 7.5 归档到 `docs/spec/archive/2026-09-25-enforce-value-type-non-null/`

---

## 已知未堵的洞（单列 follow-up，不在本变更）
- **泛型 struct 数组的 Null 槽** —— `interp/exec_array.rs` 的补丁注释自述
  「Deliberately narrow: only PRIMITIVE value params」，struct 类型参数仍走老路。
  强推 struct backing 会打坏泛型容器（`struct_generic_container: VCall: expected object, got StructRefHeap`）。

- 🟡 **泛型型参字段没有零值**（2026-09-25 归档复核时实测发现，**当时漏掉了这一格**）——
  ✅ **直接实例化那半已修**：`fix-generic-typeparam-field-zero`（分配点按实例化取零值，
  interp + JIT 同步，`value_field_zero` 已补上泛型格）。
  🔴 **仍未修的两格**：`class D : GBox<int> {}` 的**继承**字段、以及**泛型 struct** 的型参字段 ——
  两者的实参在运行期元数据里**根本不存在**（派生类的 `base_name` 不带实参；
  `StructTypeLayout` 只有 offset/kind、没有叶子声明类型），只能由编译期单调化解决。
  下面记的是**修前**的全貌，保留作现场：
  本变更的不变式「**值类型的存储槽永不含 `Value::Null`**」在泛型实例化上**今天就不成立**。

  ```z42
  class GBox<T> { public T V; public GBox() { } }

  GBox<int> g = new GBox<int>();     // C#：V == 0
  object o = g.V;                    // → null（装箱侧被 #717 的 Null 直通吞掉，无声）
  int y = g.V;                       // → int 局部里装着 Null ⇐ 不变式被破
  g.V + 1;                           // → 内部错误 `type mismatch in arithmetic: Null vs I64(1)`，catch 抓不到
  ```

  | 形态 | 现状（interp / jit 双验） |
  |---|---|
  | 非泛型类字段 / 静态字段（int/bool/char/double/long） | ✅ 零值（`value_field_zero` 钉住） |
  | 泛型类 `T` 字段，T = int / double | 🔴 `Null` |
  | 泛型类 `T` 字段，T = bool | 🔴 `Null` ⇒ **`== true` 与 `== false` 同时为 false**；`if (g.V)` 内部崩 `BrCond expects bool, got Null` |
  | 泛型 **struct** 的 `T` 字段 | 🔴 `Null` |
  | 泛型类 `T` 字段，T = 用户 struct | ✅ 零值 |
  | 泛型类里的 `T[]` 数组槽 | ✅ 零值（`ArrayNew` 走 `default_value_for_tag`） |
  | T = 引用类型 | ✅ `null` 本就是零值 |

  ⭐⭐ **还带一条 interp / jit 分叉** —— 正是 design §D2 警告的那种「tier-up 之后结果变了」：

  ```
  bool eq0 = (g.V == 0);
  interp: eq0=false ne0=true
  jit:    eq0=true  ne0=false     # 稳定复现，两次一致
  ```

  **根因不在分配点**（`alloc_object` 按 `object_storage()` 整块零初始化，是对的），而在**布局按定义算**：
  型参名落 `StructLeafKind.GcRef` 8 字节句柄 ⇒ 该槽是 ref 槽 ⇒ 零值就是 `Null`。
  这与 `generic-struct-erased-slot-value-copy` / `complete-generic-instantiation` 记的**同一个根因**
  （那两个 change 的自述：「`StructLayout._kindOf("A")`（型参名）落 `StructLeafKind.GcRef`——布局**按定义**算」）。

  ⭐ **为什么一直没人撞到**：stdlib 的泛型容器（`List<T>` / `Dictionary<K,V>`）内部存储走 **`T[]` 数组**，
  而数组那条路是**唯一按元素 tag 取零值**的（`ArrayNew` → `default_value_for_tag`）⇒ 全部正常
  （实测 `List<int>` / `Dictionary<string,int>` / `int[]` 都对）。踩得到的是**用户自己写的泛型类**
  ——「有个 `T` 字段、构造器没给它赋值」。⇒ 缺口窄，但**静默**，且违反用户从 C# 带来的预期。

  ⇒ **不从本线修**：修法 = 让 `GBox<int>` 的型参字段变真内联字节，正是那条线「单调化闭包」的内容；
  从可空线去改同一处布局判据会与在飞分支撞车。**本条作为那条线的驱动用例移交**，
  并在其完成后回头补 `value_field_zero` 的泛型格。

  ⚠️ 教训：阶段 3 判「实测已经是对的」时，探针只造了**非泛型**字段（`value_field_zero` 至今只有非泛型）。
  **「零初始化已经对了」这个结论的边界，被记成了比实测更宽的样子** —— 同一毛病在本线已是第三次
  （`??` 只搜 `src/`、`Type.GetElementType` 探针不含 `xtask test`）。判据：**写"已经是对的"之前，
  先数一遍探针覆盖了几种形态，缺的那种要么补、要么写进边界。**

## 后续 change
- `define-null-check-marks` —— 引用类型 `?` 标记 + 流分析 + 反向推导 + `Expect("理由")` + **砍 `??`（生产 69 处）与 `?.`（8 处）**
