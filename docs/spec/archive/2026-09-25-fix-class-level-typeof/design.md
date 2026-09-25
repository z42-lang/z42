# Design: 类级 `typeof(T)` 的运行期具化

## Architecture

三条 `typeof` 路径按「型参归属」分流，绑定期决定，发射期各走各的载体：

```
            typeof(X)
                │
    ┌───────────┴────────────────────────────┐
    │ X 是 Z42GenericParamType？             │
    └───────────┬────────────────────────────┘
        否      │                 是
        │       │
        │   ┌───┴──────────────────────────────────────┐
        │   │ MethodParamIndexOf(name) >= 0？（就近优先）│
        │   └───┬──────────────────────────────────────┘
        │       │ 是                       │ 否
        │       │                          │
        │       │              ┌───────────┴───────────────────────┐
        │       │              │ ClassParamIndexOf >= 0            │
        │       │              │ 且 LookupVar("this") != null？    │
        │       │              └───────────┬───────────────────────┘
        │       │                  是      │      否
        ▼       ▼                          ▼      ▼
  TypeofInstr  MethodTypeArgInsn   __class_type_arg   TypeofInstr（占位名）
  （编译期名） （frame 槽）        （实例 type_args）  （静态语境 / 兜底）
```

运行期三个载体的对照 —— **本变更只填了右下那一格**：

| | 编译期已知 | 方法级 | 类级 |
|---|---|---|---|
| **零值** `default(T)` | 常量折叠 | `MethodDefault`（frame 槽）| `DefaultOf`（实例 type_args）✅ 一直是对的 |
| **类型** `typeof(T)` | `Typeof` | `MethodTypeArg`（frame 槽）| **🆕 `__class_type_arg`（实例 type_args）** |

## Decisions

### Decision 1: 载体走 corelib builtin，不新增 IR 指令

**问题：** 类级 `typeof(T)` 需要一条「读 `regs[0].instance.type_args[i]` → 产 `Std.Type`」的
运行期通道。归档设计稿（`add-generic-methods/design.md` D3）预设的做法是「同款范式」=
像 `DefaultOfInstr` 那样新开一个 opcode。

**选项：**

- **A. 新 corelib builtin `__class_type_arg(this, idx)`**
  - 优：零格式 bump、零 fingerprint bump；`Instruction::Builtin` 在 JIT 里是按名 / BuiltinId
    的通用派发（`jit/translate/call.rs:41-64`）⇒ **JIT 白送**；自包含在 reflection 模块内。
  - 缺：比单条指令多一次 builtin 派发（与一次普通 Call 等量级；`typeof` 不在热路径）。
- **B. 新 IR 指令 `ClassTypeArg`（忠于 D3 字面描述）**
  - 优：与 `DefaultOf` 完全对称。
  - 缺：zbc 格式 bump + **两代自举纪律**（`bootstrap-seed.md`：support 先行、晚一个 nightly
    再 use）+ reader / writer / JIT translate / analysis / unsupported 全线改。**成本高一个量级，
    换来的语义与 A 完全相同。**
- **C. 纯编译期脱糖成 `this.GetType().GetGenericArguments()[idx]`**
  - 优：零运行期改动，全部复用已验证路径（**实测今天就能产 `Std.Int32`**）。
  - 缺：**让任何用了类级 `typeof(T)` 的泛型类的发射码依赖 `Std.Reflection` API** ——
    跨包名解析是已知痛点（`EmitContext._depHasFunction` 就是为这类「拼出来的名字指向
    从未发射的函数」而存在）；且 3 条指令 vs 1 条。

**决定：选 A**（User 裁决）。理由：**拿到与 B 相同的语义，但不动格式**；且**不像 C 那样
引入跨包依赖边**。`_emitMethodOf` 的抬头注释已在同一个文件里立了这个先例
（`TypeOpEmitter.z42:84-87`：「不新增 IR 指令、不 bump 格式……复用既有 Builtin opcode」）。

⚠️ **D3 的字面描述是「新 opcode」，本变更用 builtin 兑现同一语义** —— 这不是偏离设计，
而是「同款范式」在 `methodof` 之后有了更便宜的实现载体。归档时在 internals 记下这个取舍，
避免下一个人以为是漏做了 opcode。

### Decision 2: 类级分支必须加「实例语境」判据，不能盲抄 `default(T)`

**问题：** `default(T)` 的类级分支（`ExprTyper._bindDefault`）**不检查静态语境**，运行期
`default_of` 直接读 `frame.get(0)`。静态帧的 reg0 **不是 `this`，是第一个实参**。

**实测证据（本次新发现的静默 bug）：**

```z42
class Box<T> { public static string Peek(Box<int> o) { T z = default(T); return "[" + z + "]"; } }
Box.Peek(new Box<int>())   // 实测输出 [0]，正确应为 [null]
```

`default(T)` 读到了实参 `o`（一个 `Box<int>`）的 type_args。

**选项：** A — 照抄 `default(T)`（不查语境）；B — 加 `env.LookupVar("this") != null` 判据。

**决定：选 B。** 照抄会把「明显错的占位名 `T`」升级成「**看起来对的错类型**」——后者是
静默缺陷，正是这批坑点要根除的形态。判据用 `env.LookupVar("this") != null`
（`MemberResolver.Bare.z42:36` 已用同一判据判实例语境，不自创新机制）。

📌 `default(T)` 那条静默 bug **不在本 Scope 内顺手修**（Scope 纪律）；记入 tasks.md 备注，
由第二刀（继承链寻址）一并解决——它与本刀的判据不冲突，两者都在收紧同一个载体。

### Decision 3: 降级产出与 `method_type_arg` 逐字同款

**问题：** 越界 / 非对象 receiver / 空 type_args 时产什么？

**决定：** 产 `make_constructed_type(ctx, "T", &[])` —— 与 `method_type_arg` 的 OOB 分支
**逐字相同**（`exec_address.rs:97`）。理由：① 两条路的降级结果一致，用户看到的现象统一；
② 这恰好**就是今天类级 `typeof(T)` 的输出**（占位名 `"T"`）⇒ 继承 / 静态两个 Out-of-Scope
形态**行为逐字不变**，不产生任何回归面。

### Decision 4: 复用 `ClassParamIndexOf`，不自己扫 TypeParamNames

**问题：** `_bindDefault` / `_bindArrayNew` 的类级分支都是**手写 while 扫
`env.TypeParamNames`**（三处重复）。

**决定：** 本变更调 `TypeEnv.ClassParamIndexOf(name)`（`TypeEnv.z42:189`）——
它**已存在、全仓零调用**，是 `add-generic-methods` 为这一刀预留的口子。
不在本变更里把另外两处也改过去（Scope 外；且那两处走的是全表下标、对类级恰好等价）。

## Implementation Notes

### 绑定期（`TypeOpTyper._bindTypeofExpr`）

在既有方法级分支之后补类级分支——**顺序即就近优先，不能颠倒**：

```z42
if (toTarget is Z42GenericParamType) {
    int mIdx = env.MethodParamIndexOf(toTarget.Name());
    if (mIdx >= 0) { bto.IsMethodLevel = true; bto.MethodParamIndex = mIdx; }
    else {
        int cIdx = env.ClassParamIndexOf(toTarget.Name());
        // 实例语境判据（Decision 2）：静态帧的 reg0 是第一个实参，不是 this
        if (cIdx >= 0 && env.LookupVar("this") != null) {
            bto.IsClassLevel = true; bto.ClassParamIndex = cIdx;
        }
    }
}
```

### 发射期（`TypeOpEmitter._emitTypeof`）

```z42
if (e.IsClassLevel) {
    TypedReg thisReg = this._ctx.Locals.Get("this") as TypedReg;   // reg 0
    TypedReg idxReg  = this._ctx.Alloc(IrType.I32);
    this._ctx.Emit(new ConstI32Instr(idxReg, e.ClassParamIndex.ToString()));
    TypedReg cdst = this._ctx.Alloc(IrType.Ref);
    TypedReg[] cargs = new TypedReg[2];  cargs[0] = thisReg;  cargs[1] = idxReg;
    this._ctx.Emit(new BuiltinInstr(cdst, "__class_type_arg", cargs, 2));
    return cdst;
}
```

🔴 `Locals.Get("this")` 返回 null 就必须落回占位路径（防御性——绑定期判据已保证非 null，
但发射期与绑定期是两个 env，不能假设）。

### 运行期（`reflection/generics.rs`）

```rust
/// fix-class-level-typeof: 类级 typeof(T) —— 读 receiver 的 per-instance
/// type_args[idx] 产 Std.Type。镜像 exec_address::method_type_arg，但载体是
/// 实例 type_args（同 DefaultOf）而非 frame 槽。
/// 非对象 / 越界 / 空 type_args → 占位 constructed type "T"（与方法级降级逐字同款，
/// 也恰是本变更前类级 typeof 的输出 ⇒ Out-of-Scope 形态行为不变）。
pub fn builtin_class_type_arg(ctx: &VmContext, args: &[Value]) -> Result<Value> { ... }
```

⚠️ **`NativeFn = fn(&VmContext, &[Value]) -> Result<Value>`**，两个实参：`args[0]` = receiver，
`args[1]` = index（`Value::I32`/`I64`，用既有 `to_usize` 口径读）。

⚠️ **登记位置铁律**：`BuiltinId` 就是 `BUILTINS` 表下标、会被烤进 zbc
⇒ 只能追加到 `builtin_table_ext.rs` 的 `PART2` **表尾**，并按既有格式写
「`── fix-class-level-typeof (2026-09-25) — appended to preserve existing BuiltinIds ──`」注释。

## Testing Strategy

- **e2e golden**（`src/tests/generics/`，用 `Assert.Equal`，与目录既有用例同形）：
  - `class_level_typeof.z42` —— 正面 7 条：单型参值/引用、两实例互不串味、多型参对位、
    `== typeof(int)` 真假两侧、`Name`/`FullName`、数组型参、嵌套泛型型参。
  - `class_level_typeof_edges.z42` —— 边界 4 条：静态语境产占位、**静态语境带对象首参
    仍产占位**（Decision 2 的钉子）、继承基类产占位、方法级同名遮蔽仍走方法级。
- **Rust 单测**（`reflection_tests.rs`）：builtin 的三条降级路径（非对象 / 越界 / 空 type_args）
  各一条，直接调 `builtin_class_type_arg`，不经 VM。
- **活示例重放**：`examples/types/generics/gaps/typeofgap.z42` + `run.console`
  由 `xtask test examples` 逐条重放 ⇒ 修好当天该门禁会判红，必须同 PR 更新
  （**transcript 的输出与列号一律以实跑为准，不靠猜**）。
- **阴性对照（必做）**：退回修复后重建，正面用例必须判红、边界用例必须仍绿
  —— 证明两侧都有判别力，而不是交付了一道恒不响的门。
- **完整 GREEN**：`xtask test`（改了编译器 ⇒ 必须先 `xtask build sdk`，否则 examples
  门禁验的是旧二进制）。
- **自举字节不动点**：z42c / stdlib 源码零处使用类级 `typeof(T)`（全仓只有注释）
  ⇒ `xtask test compiler` 的字节不动点不应有任何漂移。**这一条要实跑确认，不是推理。**

## Deferred

- **继承链 `type_args` 装配**（第二刀，User 已裁决拆分）：`ObjNew` 携带基链实参 +
  按声明类而非扁平下标寻址（声明类可从帧的函数 owner 推导 ⇒ 零指令变更，`DefaultOf`
  白送修好）。需 fingerprint bump（缓存失效）。
  🔴 **扁平下标在 `DerivedG<U> : Box<int>` 上无解**（派生自己的 `U` 与基类的 `T` 都想占下标 0）
  —— 这就是它必须独立一刀的机制原因。
- **静态语境的类级型参**：本刀保持占位、不判红。若将来要判红需取新诊断码（破坏性变更，另议）。
