# Design: const 关键字 + 常量传播 + 死分支消除

## Architecture

```
词法/语法            语义（求值 + 登记 + 诊断）           Codegen（替换）        优化管线
────────            ──────────────────────────         ──────────────        ────────
const kw     ┐      SymbolCollector                    ExprEmitter           IrOptPipeline
FieldDecl.Mods├──▶  ├ const 字段 → ConstEval → ConstVal ─┐  const 引用          ├ ConstFold（已有）
VarDeclStmt   │     │   存 FieldSymbol.ConstVal          │  → 字面量指令   ◀────┤   ← 吃 const 替换后的字面量
  .IsConst    ┘     └ 隐式 static、不进实例字段布局        │  （查 EmitContext   └ DeadBranch（新）
                    TypeChecker/ExprTyper                │   const 表）           ← 折 br.cond(const)→br
                    ├ 局部 const → TypeEnv.ConstVal 环境  │                         + 移不可达块(ExcCount==0)
                    ├ 强制诊断（需常量初始化/不可赋值）    │
                    └ 常量表达式引用解析（已定义 const）  ◀┘
```

**核心洞察**：const 在 **codegen 阶段消失**——引用替换为字面量指令，声明不产生任何存储。因此 IR 层看不到
"const"这个概念，只看到字面量。优化管线（ConstFold）无需知道 const 的存在即自动受益。dead-branch 是唯一
"专门利用 const 后果"的新 pass，但它读的也只是普通 `ConstBoolInstr` 产出的条件寄存器，与 const 解耦。

## Decisions

### Decision 1: const 值表示 —— 复用 DefaultFold 语义，独立 ConstValue 类

**问题**：const 值需在符号表 / 局部环境里携带，供 emit 时选择正确的字面量指令。

**选项**：
- A — 直接复用 `IrGenFacts.DefaultFold`（Kind/Val/Str）。缺点：DefaultFold Kind 2 混合 int/char/bool-as-int，
  emit 时无法区分 int 与 char（发 `ConstI64` vs `ConstChar`）。
- B — 新 `ConstValue{Kind, IntVal, StrVal}`，Kind 明确区分：`Int=1 / Bool=2 / Char=3 / Float=4 / Str=5 / Null=6`。

**决定**：选 B。emit 需要精确类型 → Kind 必须区分 int/char/bool。`IntVal` 承载 int 值 / bool(0|1) /
char 码点 / f64 bits；`StrVal` 承载字符串原文（已 DecodeString）。折叠算术逻辑仍**复用** `_foldBinary`/
`_foldUnary` 的语义（镜像其溢出/除零/移位约束），只是结果装进 ConstValue。

### Decision 2: 常量求值器 ConstEval —— 按声明序 + 已定义 const 环境

**问题**：`const int B = A * 2;` 需要在求 B 时知道 A 的值。跨 const 依赖如何解？

**选项**：
- A — 拓扑排序 + 环检测（C# 做法）。功能全但复杂。
- B — 按声明序求值，const 引用只能指向**已定义**（声明在前）的 const；前向引用报"未定义 const"诊断。

**决定**：选 B（与 User 裁决"已定义 const 的常量表达式"一致）。SymbolCollector 按字段声明序逐个求值 const
字段，把已求出的存入 `FieldSymbol.ConstVal`；ConstEval 遇 `IdentExpr`/`MemberExpr` 时查已登记的 const 值，
查不到（未定义 / 非 const）→ 诊断。局部 const 同理，按语句序进 TypeEnv 环境。**无前向引用、无环** →
不需要不动点。

**ConstEval 输入**：v1 直接吃 AST `Expr`（求值发生在符号收集 / 类型检查期，binding 尚未完全就绪时也可用），
引用解析靠传入的 const 查找回调（字段：同类 const 表；局部：TypeEnv const 环境）。折叠覆盖：
字面量（int/bool/char/float/string/null）+ 一元 `- + ! ~` + 二元 算术/比较/逻辑/位（复用 `_foldBinary` 语义）。
任何非常量子表达式 → 返回"非常量" → 调用方报诊断（**不**像 `_foldDefault` 静默回落）。

### Decision 3: const 引用替换的落点 —— ExprEmitter

**问题**：在哪里把 const 引用换成字面量？

**决定**：`ExprEmitter`。const 字段引用在绑定后是 `BoundStaticGet`（或裸 `BoundIdent` 指向同类 const）；
局部 const 引用是 `BoundIdent`。在这两处 emit 前查 `EmitContext` 的 const 表：
- 命中字段 const → 按 `FieldSymbol.ConstVal.Kind` emit 对应字面量指令，**不** emit `StaticGetInstr`。
- 命中局部 const → 同理，不 emit 局部加载。
`EmitContext` 新增两张表：`_constFields`（FQN → ConstVal）+ `_constLocals`（name → ConstVal，随作用域）。
局部 const 声明语句在 `FunctionEmitter` 里**只登记 ConstVal、不 emit 存储/赋值**。

### Decision 4: const 字段不进对象布局

const 字段隐式 static 且无存储：SymbolCollector 登记其 ConstVal 后，**不**把它加入类型的实例字段列表 /
静态字段列表（不进 zpkg 的字段元数据）。这是"跨包看不到 const 值"的根因（→ Out of Scope）。

### Decision 5: dead-branch pass —— 折叠 + 可达性移除，异常表铁律

**pass 逻辑**（`IrDeadBranch.Run(func)`）：
1. 遍历每块终结子：若为 `BrCondTerm(cond, T, F)` 且 `cond` 的产出指令是 `ConstBoolInstr(v)` →
   替换终结子为 `BrTerm(v ? T : F)`。（**始终安全**：条件跳转变无条件跳转，不移块。）
2. 若 `func.ExcCount == 0`：从 entry 块做可达性 BFS（沿 Br/BrCond/Ret 后继），移除不可达块。
   若 `func.ExcCount > 0`：**跳过移块**（异常表的 TryStart/TryEnd/CatchLabel 隐式边不在终结子 CFG，
   贸然移块会删掉 handler 可达的块 → miscompile；镜像 IrLicm 的 `ExcCount>0` 整体跳过铁律）。

**为何安全**：步骤 1 纯局部改写；步骤 2 仅在无异常表时做，此时终结子 CFG 是完整 CFG，可达性精确。

**与 ConstFold 的顺序**：dead-branch 排在 ConstFold **之后**（ConstFold 可能把某些比较折成 `ConstBoolInstr`，
制造更多常量条件）。dead-branch 移块后可再触发一轮 DCE（现有 pass）清理孤立指令——靠管线既有 pass 顺序。

**Opt 位**：`Opt.DeadBranch`（下一个空位，当前 `PureCall=512` → `DeadBranch=1024`，`All` 相应更新）。
单独门控；dump/golden 默认 optSet 减 `DeadBranch`（同 Inline/StackAlloc/PureCall，防既有 golden 漂移）。

### Decision 6: 无格式 bump

const 全在 codegen 替换为既有字面量指令；dead-branch 是 ZbcWriter 前纯 IR 变换。zbc/zpkg 字节即字面量，
无新 opcode、无新字段 → **不 bump**。（同 readonly #124：编译期优化提示 vs 运行期语义标志。）

### Decision 7: 两-nightly 纪律

`const` 是新语法。本 change 只落"支持"：parser 接受 `const`，z42c/stdlib/xtask **源码本轮不使用**。
测试 fixture / stdlib bench **可立即用**（由当前自建 z42c 编译，非 seed）。晚一个 nightly 后另开 follow-up
把 `TokenKind`/`DiagnosticCodes` 等 `static int` 常量迁到 `const`（享受内联收益）。
`xtask test bootstrap` 验证上一 nightly z42c 仍能编当前源（本轮源不含 const → 必过）。

## Implementation Notes

- **ConstValue.Kind**：`Int=1 / Bool=2 / Char=3 / Float=4 / Str=5 / Null=6`。
- **emit 映射**：Int→`ConstI64Instr(dst, IntVal.ToString())`；Bool→`ConstBoolInstr(dst, IntVal!=0)`；
  Char→`ConstCharInstr(dst, …)`；Float→`ConstF64Instr(dst, IntVal /*bits*/)`；
  Str→`ConstStrInstr(dst, 串池 idx)`；Null→`ConstNullInstr(dst)`。
- **诊断码**：`DiagnosticCodes` 新增 `ConstNeedsInit` / `ConstNotConstantInit` / `ConstAssign` /
  `ConstExprBadRef`（E04xx 段，具体号取当前空位）。
- **赋值检查**：`ExprTyper._bindAssign` 已有 `_checkReadonlyAssign`；const 赋值检查并入同处（先判 const 再判
  readonly，避免双诊断），覆盖 `BoundStaticGet`（字段 const）与裸 `BoundIdent`（局部 const）两路。
- **IrDeadBranch** 需 `ConstBoolInstr` 值查询：块内线性扫建 `defReg → ConstBoolInstr` 映射（或复用
  IrOptInfo 现有常量收集）。移块后同步 `BlockCount` 并保留 entry 为首块。
- **局部变量非块 shadow 陷阱**（compiler-z42c.md）：ConstEval / DeadBranch 内嵌套块局部变量名避开外层同名。

## Deferred / Future Work

（roadmap Deferred Backlog Index 索引本段。）

### const-future-crosspkg: 跨 zpkg const
- **触发原因**：const 无字段元数据（不进 zpkg 字段段）→ 别的包看不到其值。
- **前置依赖**：zbc/zpkg 格式 bump，把 const 值写进导出元数据（`IrFieldDesc` / TSIG / ImportedSymbolLoader）。
- **触发条件**：跨包具名常量成为高频需求时。
- **当前 workaround**：跨包需要常量时用普通 `static` 字段（有存储、可跨包读，无 const 内联收益）。

### const-future-crossscope-init: 跨类 / 跨作用域 const 初始化器引用
- **触发原因**：v1 按声明序 + 局部/同类 env 求值——字段初始化器只引**同类**已定义 const、局部只引**作用域内**局部 const。
- **前置依赖**：模块级 const 依赖图 + 拓扑求值（或跨类查表）。
- **触发条件**：`const int X = Other.Y * 2` 这类跨类初始化成为需求时。
- **当前 workaround**：把依赖值复制成同类/本地 const，或用非 const 表达式（放弃编译期常量性）。

### const-future-nonprimitive: const 引用 enum 成员 / const 数组 / const 对象
- **触发原因**：v1 仅原始类型常量（int/bool/char/float/string/null）；ConstValue 不表示聚合/枚举。
- **前置依赖**：ConstValue 扩展 + enum 成员常量求值接入。
- **触发条件**：`const` enum / 常量数组成为需求时。

### const-future-deadbranch-exc: ExcCount>0 函数的死块移除
- **触发原因**：CFG 铁律——异常隐式边（try→catch/finally）不在终结子 CFG，移块不安全 → v1 对有异常表的函数只折 `br.cond→br`、不移块。
- **前置依赖**：把异常边纳入 CFG（design 级），或从异常表标签补可达根。
- **触发条件**：热点常量条件恰在 try 块内、且死块体积可观时。
- **当前 workaround**：折叠已消除条件跳转开销；死块残留只是体积（不执行），影响有限。

## Testing Strategy

- **单元（z42c.semantics codegen_tests）**：
  - const 字段/局部 → 字面量替换（断言 emit 出 ConstI64/Bool/Char/F64/Str，无 StaticGet）
  - `const B = A*2` 求值 20
  - dead-branch：恒假/恒真折叠 + 移块（ExcCount==0）；有异常表只折不移（构造 try/catch）
  - 单独开 `Opt.DeadBranch` 正确、逐字节稳定
- **解析（z42c.syntax parse test）**：const 字段 / 局部 / 修饰符组合 Dump
- **Golden e2e**：
  - `const_fold_propagation`：const 折进算术 / 循环边界，输出恒定
  - `const_dead_branch`：`if(const false)` 不执行、`if(const true)` 执行
  - `const_basic`：字段 + 局部 const 端到端语义
  - `const_errors`：4 类诊断
- **诊断**：非常量初始化 / 缺初始化 / 赋 const / 非 const 引用
- **GREEN gate**：`xtask test`（全 stage）。self-host 不动点：本 change 改 codegen 输出 → 当次 gen1≠gen2
  破一代（D7），重建 gen2==gen3 自愈 → `xtask test compiler` 跑两遍收敛（预期，非 bug）。
- **A/B 量测**：const 折叠 bench（`--no-opt const-fold,dead-branch` vs 默认，project build，interp 计时）。
