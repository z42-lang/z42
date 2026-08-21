# Design: 泛型方法端到端（方法级 type_args 运行期通道，M1）

## Architecture

```
调用点  Foo<MyType>(a)
          │  编译期：TypeChecker 绑定方法 type_args=[MyType]（解析成 FQ 具体名）
          ▼
IR      Call{ target=Foo, args=[a], method_type_args=["MyType"] }        ← zbc 新字段（格式 bump）
          │
          ▼ interp/jit 建帧
Frame   { regs=[…], method_type_args: ["MyType"] }                       ← 新槽（镜像 instance.type_args）
          │
方法体  typeof(T) / new T() / default(T)
          │  IR：MethodTypeArg{dst, param_index}（+ 后续 activator/default）
          ▼
运行期  make_type_from_name(frame.method_type_args[param_index]) → 具体 Std.Type
```

**核心对称**：类型形参的具化 = 执行上下文携带具体类型名。
- **类级**（已有）：`Object.instance.type_args` 携带；`DefaultOfInstr` 读 `regs[0]`。
- **方法级**（M1 新增）：`Frame.method_type_args` 携带；新指令读 frame 槽。

## Decisions

### Decision 1（Q1）：Call 指令携带 `method_type_args` 字段（而非独立 SetMethodTypeArgs 指令）

**问题**：方法 type_args 如何从调用点送达 callee frame？
**选项**：
- A — 扩 `Call` / `VCall` 指令加 `method_type_args: Box<[String]>` 字段；建帧时拷入 frame。
- B — Call 前发一条 `SetMethodTypeArgs` 指令，写入"待用槽"，被下一条 Call 消费。
**决定**：**A**。type_args 与 Call 是同一逻辑动作、一对一绑定；B 引入隐式跨指令耦合（中间抛异常/跳转会污染待用槽），更脆。代价=Call/VCall 编码 + 解码各加一段（格式 bump 已在预期内）。**非泛型调用该字段为空数组**（零额外语义）。

### Decision 2（Q2）：方法级形参解析用**新指令**，不复用类级 index 指令

**问题**：`typeof(T)`/`new T()`/`default(T)` 方法级如何与类级区分？类级 `DefaultOfInstr(idx)` 读 `regs[0].instance.type_args`；方法级要读 `frame.method_type_args`——**载体不同**。
**选项**：
- A — 给现有指令加 1bit scope 标志（class/method）。
- B — 新增方法级专用指令。
**决定**：**B**。载体不同、解析路径不同，1bit 标志会让一条指令承担两套语义（违反设计完整性）。新增一条**统一物化指令** `MethodTypeArgInsn{dst, param_index}` → 运行期 `make_type_from_name(frame.method_type_args[idx])` 产**具体 `Std.Type`**。在此之上：
- `typeof(T)` = 直接 `MethodTypeArgInsn`。
- `new T()` = `MethodTypeArgInsn` → 复用 `__activator_create`（builtin，已存在）构造实例。
- `default(T)` = `MethodTypeArgInsn` 产 Type → 复用 `default_value_for` 取零值（新增一个"按 Type 取默认值"的薄封装，或 `MethodDefaultInsn{dst, param_index}` 直接读槽名 → `default_value_for(tag)`，与类级 `DefaultOfInstr` 同款只是载体换 frame）。

> **最小指令集**：`MethodTypeArgInsn`（物化 Type，喂 typeof + new）+ `MethodDefaultInsn`（零值，镜像 `DefaultOfInstr` 但读 frame 槽）。两条即可。`new T()` 不单列指令——codegen 展成 `MethodTypeArgInsn` + activator 调用。

### Decision 3（Q3）：方法级 `typeof(T)` 建全新解析路径（类级现状不动）

**勘察结论**：类级 `typeof(T)` 当前**也不**解析为具体类型（`Typeof` insn → `make_constructed_type("T")` 产占位名 Type；`typeof.z42` 测试仅覆盖 `typeof(具体)`）。只有 `default(T)` 类级有真解析（`DefaultOfInstr`）。
**决定**：M1 **只**为方法级建 `typeof(T)`→具体 的解析（经 `MethodTypeArgInsn` + frame 槽）；**类级 `typeof(T)` 的具体化不在本 Scope**（无 serde 依赖、避免 Scope 蔓延）。若后续需要，另开 change 用同款范式补类级。

### Decision 4（Q4，User 裁决"纳入"）：`typeof(T)` + `new T()` + `default(T)` 三者全纳入 M1

三者共用 `Frame.method_type_args` 载体，建好载体后边际成本低。一次交付完整"方法级类型参数"能力，而非只够 serde 用的 `typeof(T)`。

### Decision 5：`method_type_args` 存**解析后的具体 FQ 类型名**（String），非索引/句柄

**理由**：与类级 `instance.type_args`（存类型名字符串）一致；`make_type_from_name` / `default_value_for` 均以名/tag 为输入。调用点编译期已知具体类型 → 直接解析成 FQ 名编码进 Call。**若调用点实参本身是调用者的类型参数**（嵌套泛型方法调用），M1 要求为具体名（serde 招牌 `Deserialize<MyType>` 满足）；调用者形参转发留后续（Out of Scope）。

### Decision 6：support 先行——本变更零 z42c/stdlib 泛型方法使用

per bootstrap-seed.md：本变更只落**支持**（编译器能编 + VM 能跑），z42c / stdlib / xtask 源码**不**写泛型方法。`Deserialize<T>`（M2）在本支持随下一 nightly 发布后落地。格式 bump 由 ci-bootstrap 两代自举吸收（已验证机制）。

## Implementation Notes

### 前端：`<` 歧义消解（Parser）

调用点 `expr < ...` 需判定是"泛型调用类型实参"还是"小于比较"。采用 **C# 式有限前瞻**：`名<` 后尝试解析 `类型列表 >` 且其后紧跟 `(` → 判为泛型调用；否则回退为比较。回退必须零副作用（保存/恢复 token 位置）。**验证既有 golden 零漂移**是本决策的硬门禁（`a < b > c` 等表达式不得被误判）。

### 前端：绑定与解析（TypeChecker / ExprEmitter）

- `CallExpr.TypeArgs`（`TypeExpr[]`）→ TypeChecker 解析成具体 `Z42Type`，arity 校验，`where` 约束校验（复用 `ConstraintChecker`）。
- 方法体内 `BoundTypeof` / `BoundDefault` / `new` 若 Target 是**方法级** `Z42GenericParamType` → ExprEmitter 发方法级指令，`param_index` = 该形参在方法 `TypeParams` 中的序号。
- 判别"方法级 vs 类级形参"：形参名在**当前方法** `TypeParams` 命中 → 方法级；在**类** `TypeParams` 命中 → 类级（走现有路径）。方法级优先（就近作用域）。

### IR/格式：编解码 + 版本 bump

- `Call` / `VCall` 加 `MethodTypeArgs: string[]`（IrInstr.z42）；`ZbcWriter`/`ZbcReader` 编解码（空数组=0 长度前缀）。
- 新指令 `MethodTypeArgInsn` / `MethodDefaultInsn`（opcode + `param_index` u8）。
- `ZbcVersion.Minor` + `ZpkgWriter.Minor` 递增（version-bumping.md 全 checklist：C# 侧若有镜像、golden fixture regen、strict-pin 断言）。

### 运行期：Frame 槽 + 建帧 + 执行

- `interp/mod.rs` `Frame` 加 `method_type_args: Box<[String]>`（默认空）。
- `exec_call.rs` 建帧：从 Call insn 的 `method_type_args` 拷入新 frame（若含调用者形参名——M1 视为具体名，不转发解析）。
- `exec_address.rs`（或新 `exec_typearg.rs`）：`MethodTypeArgInsn` → `make_type_from_name(frame.method_type_args[idx])`；`MethodDefaultInsn` → `default_value_for(frame.method_type_args[idx])`。OOB/空 → graceful（`typeof` 产占位、`default` 产 Null），镜像类级容错。
- `zbc_reader.rs`：解码 Call 新字段 + 新指令。
- **jit**（`translate.rs` + `frame.rs`）：镜像——jit frame 加 `method_type_args`；Call 传递；新指令走 helper（interp 全绿后再补，per workflow interp-first；但同一 change 内交付，因是 lang/ir/vm 完整门禁）。

## Testing Strategy

- **golden（interp）**：`src/tests/e2e/generic-methods/`——
  - `typeof(T)` = 用户类真句柄（`.GetFields()` 可枚举）+ 基元 + 与直接 `typeof` 一致；
  - `new T()` 构造实例；`default(T)` 引用→null / 值→零值；
  - 多参 `<K,V>`；arity 错诊断；`where` 约束违背诊断；
  - `<` 歧义：比较表达式不被误判（专门 fixture）。
- **byte-identical / 自举**：`xtask test compiler`（z42c 自建，本变更不使用泛型方法 → gen 稳）；`xtask test bootstrap`（确认上一 nightly 仍能编当前源——support 先行守住）。
- **格式**：golden zbc fixture regen + strict-pin 断言；cold 路径以 CI 两代自举为准（本地只验 warm）。
- **jit**：CI `test-vm-jit` 覆盖；本地 `xtask test e2e --mode jit` 抽验。
- 完整 GREEN gate（`xtask test` 全 stage）以 CI 为准（格式 bump 冷路径本地不可验，per bootstrap-seed.md）。

## Deferred / Future Work

### generic-methods-future-reflective-invoke: 反射式 `MakeGenericMethod().Invoke()`
- **来源**：本 change Out of Scope + plan-generic-reflection ③。
- **触发原因**：M1 只做直接调用；反射式 invoke 填充 `method_type_args` 的路径不同（从 `MethodInfo` 运行期实参而非编译期调用点）。
- **前置依赖**：M1 的 Frame `method_type_args` 载体（已建）。
- **触发条件**：泛型反射三件套收口时。

### generic-methods-future-type-inference: 方法 type_args 推断
- **来源**：本 change Out of Scope。
- **触发原因**：M1 要求显式 `Foo<T>(x)`；从实参推断 `T` 需类型统一。
- **触发条件**：泛型人机工学打磨阶段。

### generic-methods-future-classlevel-typeof: 类级 `typeof(T)` 具体化
- **来源**：Decision 3。
- **触发原因**：类级 `typeof(T)` 当前产占位名；M1 只补方法级。
- **前置依赖**：可复用 M1 的物化范式（载体换 `instance.type_args`）。
- **触发条件**：有类级 `typeof(T)`→具体 的真实需求时。
