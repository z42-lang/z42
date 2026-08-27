# Spec: 模式匹配 A2 —— or / `@` / `..=` / 关系

> 归属：语言规范 · 模式匹配（A1 spec 之增量）。A2 在 A1 文法上加四个组合子。

## 文法（A2 增量，嫁接 A1）

```
Pattern      := OrPattern
OrPattern    := PrimaryPattern ( '|' PrimaryPattern )*        // 仅 switch 臂；is 不适用
PrimaryPattern :=
    '_'                                    // 通配（A1）
  | Constant ( '..=' Constant )?           // 常量（A1）/ 闭区间范围（A2）
  | RelOp Constant                         // 关系模式（A2）：> >= < <=
  | ident '@' PrimaryPattern               // @ 绑定（A2）
  | Type                                   // 类型（A1）
  | Type ident                             // 类型 + 绑定（A1）
  | Type '(' Pattern (',' Pattern)* ')'    // 位置/record 解构（A1）
  | Type? '{' field ':' Pattern (',' ...) '}'  // 属性（A1）
  | ident                                  // 裸绑定（A1，bind 期定性）
RelOp        := '>' | '>=' | '<' | '<='
Constant     := literal | dotted-name      // 1 / "s" / 'c' / true / null / -5 / Color.Red
```

- **应用位点**：`switch` 语句 case（`case Pattern [if guard]:`）、`switch` 表达式 arm
  （`Pattern [if guard] => expr`）收全部形态；`is` 表达式（`x is Pattern`）**仅** `..=` / 关系
  （不含 or `|` 与 `@`）。
- **守卫**：`if <bool-expr>`，位于模式之后、`:`（stmt）或 `=>`（expr）之前。与 or/@/范围/关系正交组合。

## 语义

### or-模式 `P1 | P2 | ...`

- 从左到右尝试各子模式，**任一匹配即整体匹配**（短路）。
- **A2 约束**：子模式**不得引入任何绑定**（裸绑定 / `T x` / `@` / 位置·属性内的子绑定 / 嵌套绑定均禁）。
  违反 → 编译错误「or-模式的子模式暂不支持绑定」。
- 典型：`case 1 | 2 | 3:`、`case Circle | Square:`、`case 1 ..= 5 | 10 ..= 20:`。

### `@` 绑定 `name @ P`

- 绑定 `name` 到**整个 subject**，**且** subject 须匹配子模式 `P`（`name` 与 `P` 内绑定同时生效）。
- `name` 的静态类型 = subject 静态类型。子模式 `P` 不含顶层 or（`@` 后接 PrimaryPattern）。
- 典型：`case p @ Point(0, y):`（`p` = 整点，`y` = Y）；嵌套 `case Line(a @ Point(_, _), _):`。

### `..=` 闭区间范围 `lo ..= hi`

- 匹配当且仅当 `subj >= lo && subj <= hi`（**含**两端点）。`lo`/`hi` 为编译期常量表达式。
- **仅可比较基元**：整数族（int/long/short/byte/sbyte/uint/ulong/ushort）、浮点族（float/double）、`char`。
  subject 静态类型非可比较基元 → 编译错误。
- 典型：`case 1 ..= 5:`、`case 'a' ..= 'z':`、`case 0.0 ..= 1.0:`。

### 关系模式 `> v` / `>= v` / `< v` / `<= v`

- 匹配当且仅当 `subj <op> v`。`v` 为编译期常量。可比较基元约束同 `..=`。
- 典型：`case > 0:`、`case <= 100:`、`case >= 'a':`。

## 降级（lowering，无格式 bump）

全部 lowering 到既有 IR，无新指令、无 zbc/zpkg 变更：

| 模式 | IR |
|------|----|
| `P1 \| P2` | 依次 `EmitMatch`：alt 失败 → 下一 alt 块；末 alt 失败 → fail |
| `name @ P` | `Locals[name] = subj` + `EmitMatch(P)` |
| `lo ..= hi` | `Ge(subj, lo)` → BrCond → `Le(subj, hi)` → BrCond(match/fail) |
| `> v` | `Gt(subj, v)` → BrCond(match/fail)（`>=`→`Ge`、`<`→`Lt`、`<=`→`Le`） |

## 新词法记号

| 记号 | 文本 | 说明 |
|------|------|------|
| `At` | `@` | `@` 绑定。此前无用途，z42c 源无裸 `@`。 |
| `DotDotEq` | `..=` | 闭区间范围。须先于 `..`（`DotDot`）判定。 |

## 不在 A2（后续）

- or-模式**带绑定**（各 alt 绑定集一致性 + 合流寄存器）。
- `is` 表达式中的 or / `@`。
- 解构声明 `Point(x, y) = p`（迭代 B）、穷尽性诊断（C）、`with`（D）、`init`（E）、元组（F）。
- 半开区间 `..` / `lo..` / `..hi`（A2 只闭区间 `..=`）。
