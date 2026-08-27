# Design: Rust 风格模式匹配核心（A1）

## Architecture

模式匹配横跨编译器三层，每层加一族**模式节点**，与现有 switch/is 节点并行，用一条**递归下降 lowering**
收口到既有 IR。数据流：

```
源码  case Point(0, y) if y > 0:
  │  syntax 层
  ▼  PatternParser._parsePattern → Pattern AST（PositionalPattern{ type=Point, elems=[ConstantPattern 0, BindingPattern y] }）
  │  semantics 层（bind）
  ▼  PatternBinder.Bind → BoundPattern（resolved 类型 + 字段索引 + 绑定名进 TypeEnv）+ 守卫 BoundExpr
  │  semantics 层（emit）
  ▼  PatternEmitter.Emit → 既有 IR：IsInstance(subj,Point) → BrCond → FieldGet subj.X →Eq 0 → FieldGet subj.Y →bind y → guard → body
```

三个**应用位点**（switch-stmt / switch-expr / is）共用 `PatternParser` / `PatternBinder` / `PatternEmitter`，
差异只在**外壳**：switch 是 case 链（失败落下一 case）、is 是单模式布尔结果（+ true 分支绑定可见）。

### 模式节点三层（与 switch/is 现有节点并行）

| 层 | 文件 | 节点 |
|----|------|------|
| AST | `Pattern.z42`（NEW） | `Pattern`(abstract) → `WildcardPattern` / `ConstantPattern(Expr)` / `TypePattern(TypeExpr, string bind)` / `PositionalPattern(TypeExpr, Pattern[] elems)` / `PropertyPattern(TypeExpr?, string[] names, Pattern[] pats)` / `BindingPattern(string name)` |
| Bound | `BoundPattern.z42`（NEW） | 镜像层级，携 resolved `Z42Type`、字段索引 `int[]`、绑定名；常量携 `BoundExpr` |
| —（复用节点）| `Stmt.z42`/`Ast.z42`/`BoundStmt.z42`/`BoundExprOp.z42` | `SwitchCase`/`SwitchArm`/`IsExpr` 的 `pattern` 字段由 `Expr`/`BoundExpr` 改为 `Pattern`/`BoundPattern`；`SwitchCase`/`SwitchArm` 加 `guard` |

## Decisions

### Decision 1: 扩 `switch`、不引入 `match` 关键字

沿用 C# 基底（switch-stmt 已有 `hasPattern` 标志、switch-expr 已有 `p => b` 文法），只把 case 的**解析**从
`_parseExpr(0)` 换成 `_parsePattern`，语义/emit 收口到新引擎。好处：零新关键字、与现存 switch 一致、守卫复用 `if`。
（已裁决，见记忆。）

### Decision 2: 裸标识符歧义 = 解析期按名字形状 + bind 期按类型表消解（无 `var`）

z42 正去掉 `var` 关键字，故模式绑定用 **Rust 裸标识符**，不写 `var x`。歧义分两级消解：

- **解析期（PatternParser）** 按**名字形状**先分流，不查符号表：
  - 字面量 token → `ConstantPattern`。
  - `_`（文本恰为下划线）→ `WildcardPattern`。
  - **点分名** `A.B[.C]` → `ConstantPattern`（枚举/常量，如 `Color.Red`，构造 MemberAccess 表达式）。
  - 名字后跟 `(` → `PositionalPattern`；后跟 `{` → `PropertyPattern`；后跟另一 ident → `TypePattern(type, bind)`。
  - **单名**、后面不接上述 → `BindingPattern(name)`（待 bind 期定性）。
- **bind 期（PatternBinder）** 对 `BindingPattern(name)`：`name` 解析命中**类型名** → 视为 `TypePattern`（匹配
  任意该类型实例、不绑定）；否则 → **新绑定**（引入 `TypeEnv`）。裸名遮蔽类型名 → 后置 warning（C 迭代，A1 不做）。

> 与 C# 一致：常量必须限定名/字面量，简单名恒为绑定或类型——不会把裸名当局部 `const` 匹配。

```mermaid
flowchart TD
    P[模式位置的一个 token 序列] --> L{是字面量?<br/>1 / \"s\" / null}
    L -- 是 --> C1[ConstantPattern]
    L -- 否 --> U{文本是 _?}
    U -- 是 --> W[WildcardPattern]
    U -- 否 --> D{带点号?<br/>A.B.C}
    D -- 是 --> C2["ConstantPattern<br/>(枚举/常量 Color.Red)"]
    D -- 否 --> N[解析名字路径] --> K{后随什么?}
    K -- "("  --> POS[PositionalPattern]
    K -- "{"  --> PROP[PropertyPattern]
    K -- "另一个 ident" --> TB["TypePattern(type, bind)<br/>Point p"]
    K -- "其它(逗号/冒号/=>...)" --> BIND["BindingPattern(name)<br/>⬅ 单名，待 bind 期定性"]

    BIND -.->|bind 期查类型表| R{name 是类型名?}
    R -- 是 --> TP["类型模式<br/>IsInstance, 不绑定"]
    R -- 否 --> NB["新绑定<br/>匹配任意值 + 绑定"]

    style BIND fill:#fff3cd,stroke:#d39e00
    style R fill:#fff3cd,stroke:#d39e00
```

**关键**：解析期规则把「常量」这条路彻底交给字面量 / 点分名，**裸单名永不作常量** → 歧义收窄成「类型 vs 绑定」
二选一，再由 bind 期唯一地按类型表定性。这与 Rust（裸小写名恒为绑定、已知变体名才作模式）、C#（裸单名恒为
designation、常量须限定）都一致。

### Decision 3: record 位置模式走内建主构造器字段（无 Deconstruct、无 out）

`Point(a, b)` 的 lowering：`IsInstance(subj, Point)` 通过后，第 i 个子模式对 `subj` 的**第 i 个声明序字段**
（`Z42ClassType.OwnFieldNames[i]`）匹配。**位置↔字段名映射由 record 主构造器声明序内建提供**，用户不写任何
解构方法。约束（PatternBinder 校验）：位置模式的类型必须 `IsRecord`（非 record 用位置模式 → 诊断错误，指向
「位置解构仅限 record」）；子模式数 == `OwnFieldCount`（arity 不符 → 错误）。泛型 record 的位置解构 A1 不支持（defer）。

### Decision 4: lowering = 递归「test + bind」，短路 BrCond；复用 record 程序的**直读字段**范式

`PatternEmitter.Emit(subj, pattern, onFail)` 递归下降，产出**布尔匹配 + 副作用绑定**：

| 模式 | test | bind |
|------|------|------|
| Wildcard | 恒真（不 emit 测试） | — |
| Constant | `Eq(subj, const)` | — |
| Type `T` / `T x` | `IsInstance(subj, T)` | 命中后 `x = subj`（静态视作 T） |
| Positional `T(p_i)` | `IsInstance(subj, T)` ∧ 逐 `p_i` 对 `FieldGet subj.f_i` 递归 | 子模式绑定 |
| Property `T{F:p}` | `IsInstance(subj, T)`（T 可省则跳过）∧ `p` 对 `FieldGet subj.F` 递归 | 子模式绑定 |
| Binding `x` | 恒真 | `x = subj` |

- **短路**：类型/字段测试用 `BrCond` 到 `onFail`（下一 case / is-false 分支）——测试失败不读字段，避免 null/类型错。
- **⚠️ jit 安全（record 程序血泪教训）**：位置/属性模式读字段用 **`FieldGet [owner=T, field=f_i] subj`（显式 owner
  类型、直读 subject 寄存器）**，**不** emit `as_cast subj→T` 再 `field_get`——后者被 jit 误编（record `Equals`
  合成踩过，[[record-value-semantics-program]]）。范式照抄 `FunctionEmitter.EmitSynthEqualsResult`
  （`IsInstance` 后直读字段）。
- **绑定落点**：绑定变量分配局部/寄存器，写入后在 arm body / 守卫 / is-true 分支可读（binder 已注册进 `TypeEnv`）。

### Decision 5: ⚠️ ConstantPattern 必须 byte-identical 现状（自举不动点铁律）

自举**不动点**（gen1 == gen2）要求：OLD seed z42c（旧 switch emit）与 NEW z42c（本 change 的 emit）对
**当前 z42c 源码里的常量 switch** 产出**逐字节相同**的 bytecode——否则 gen1（OLD 编）≠ gen2（NEW 编），
自举断链。因此 `PatternEmitter` 的 **ConstantPattern lowering 必须复刻现有 `_emitSwitchExpr`/`_emitSwitch`
的指令序 + 寄存器分配顺序**（`Eq(subj, pat)` + `BrCond` 链，一字不差）。这是本 change 的**头号 IMPL 约束**：
z42c/stdlib/xtask 源**只用常量 switch**，其发码必须不变；新模式语法**只在 e2e 测试文件出现**（fresh z42c 编）。

### Decision 6: 无格式 bump、单 PR、两-nightly 纪律

纯编译期 lowering 到既有 IR（`IsInstance`/`Eq`/`FieldGet`/`BrCond`），**无 zbc/zpkg 格式变更、无新 runtime**。
新语法**只被测试文件使用**、z42c 自身源不用 → **上一 nightly 的 z42c 仍能编当前源**（两-nightly 纪律满足），
单 PR 落地。改动 parser/codegen 后跑 `xtask test bootstrap` 验无越界。

## Implementation Notes

- **AST/Bound 字段迁移**：`SwitchCase`/`SwitchArm` 的 `pattern` 从 `Expr`→`Pattern`、`BoundSwitchCase`/
  `BoundSwitchArm` 从 `BoundExpr`→`BoundPattern`，并加 `guard`/`Guard`。所有构造点/读取点同步（parser 造、
  binder 读造、emitter 读）。`default` case：`hasPattern=false`，无 `Pattern`（保持）。
- **guard 解析**：pattern 之后 peek `if` → 消费 + `_parseExpr`。switch-stmt 在 `:` 前，switch-expr 在 `=>` 前。
- **is 扩展**：`IsExpr` 由 `(TypeExpr, string bind)` 改为持 `Pattern`；`x is T`/`x is T v` 退化为
  `TypePattern`，**保持现有 is 发码**（byte-identical 现状，同 Decision 5 精神）；`x is Point(a,b)` 走新引擎。
- **文件行数**：新引擎拆 3 文件（parser/binder/emitter）各控 < 500 行；接线点只加派发不塞逻辑。
- **绑定作用域实现**：binder 为每个 arm 造子 `TypeEnv`（或进入/退出作用域），把模式绑定注册进去；switch-expr
  各 arm、is-true 分支独立作用域，互不泄漏。

## Testing Strategy

- **e2e 自检**（`src/tests/pattern-matching/pattern_core.z42`，`Assert` 范式、空 stdout）：
  通配 / 常量（int·string·char·bool·null·枚举限定名）/ 类型 `T`·`T x` / 位置 `Point(x,y)`·`Point(0,y)` /
  属性 `{X:0}` / 嵌套 `Line(Point(x,_),_)` / 守卫 `if` / 绑定作用域可见性 —— 每种在 **switch-stmt + switch-expr
  + is** 三位点各验一次。
- **回归**：现有 `switch*` 用例 + `is` 用例必须全绿且**发码不变**（byte-identical）。
- **⚠️ jit 双验**：`xtask test e2e --file pattern_core`（interp + jit 双跑）——位置/属性模式 emit
  `IsInstance`+`FieldGet`+`BrCond`，interp 过 ≠ jit 过，必 `--mode jit` 复验（record 程序教训）。
- **自举不动点**：本机 z42vm 退出期挂起 → 交 CI（含 test-vm-jit + gen1==gen2 不动点 + `test bootstrap` 越界门）。
  本机靠 `cargo test --lib`（不涉）+ 单 `--file` e2e + `xtask build compiler`（catch z42 编译错）。
