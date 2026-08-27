# 模式匹配（Rust 风格结构化模式）

z42 的 `switch`（语句 + 表达式）与 `is` 支持一套统一的**结构化模式**：通配、常量、类型、
**record 位置解构**、属性、嵌套、裸绑定，配 `if` 守卫。record 是积类型数据载体，模式匹配是消费它的
天然方式——`Point(x, y)` 直接按主构造器声明序绑定字段，**无需 `Deconstruct` 方法、无需 `out` 参数**。

> 本页对应 A1（结构化核心）。or-模式 `|` / `@` 绑定 / `..=` 范围 / 关系模式（A2）、解构声明
> `Point(x,y) = p`（B）、穷尽性诊断（C）、`with`/`init`（D/E）为后续独立特性。

## 模式文法

```
Pattern :=
    _                              // 通配：匹配任意、不绑定
  | <literal>                      // 常量：1 / "s" / 'c' / true / null
  | <Qualified.Name>               // 常量：枚举/限定常量（Color.Red）
  | <ident>                        // 裸绑定（命中类型名则=类型模式，见「裸名歧义」）
  | <Type> <ident>                 // 类型测试 + 绑定：Point p
  | <Type> ( <Pattern>, ... )      // 位置模式（record 解构）：Point(x, y) / Point(0, y)
  | <Type>? { <field>: <Pat>, ...} // 属性模式：Point { X: 0, Y: y } / { X: 0 }
                                   // 嵌套：Line(Point(x, _), _)
```

三个应用位点共用同一套文法：

```z42
// ① switch 语句
switch (shape) {
    case Circle(r) if r > 0:  area = 3.14 * r * r;  break;
    case Rect(w, h):          area = w * h;          break;
    default:                  area = 0;              break;
}

// ② switch 表达式
string s = p switch {
    Point(0, 0)          => "origin",
    Point(x, y) if x > 0 => "right",
    Point(x, y)          => "left"
};

// ③ is 表达式（绑定在 true 分支可见）
if (obj is Point(a, b)) { use(a, b); }
```

绑定用 **Rust 裸标识符**（z42 无 `var`）；守卫用 **`if`**（`pattern if guard`）。

## 裸名歧义：解析期形状 + 绑定期类型表

模式位置的一个裸标识符可能是「类型测试」「新绑定」或「常量」。z42 分两级唯一消解（同 Rust / C#）：

1. **解析期**按名字形状分流（不查符号表）：字面量→常量；`_`→通配；**点分名**（`A.B`）→常量；
   名字后跟 `(`→位置、`{`→属性、后跟另一 ident→类型+绑定；**单个裸名**→留待绑定期。
2. **绑定期**对单裸名查类型表：**命中类型名 → 类型模式**（`IsInstance`，不绑定）；**否则 → 新绑定**。

**关键**：常量这条路只留给字面量 / 点分名——裸单名**永不作常量**，歧义收窄成「类型 vs 绑定」二选一。
所以常量匹配必须写限定名（`Color.Red`）或字面量。

## record 位置解构

`Point(a, b)` 中位置 ↔ 字段的映射由 **record 主构造器声明序内建提供**（`Z42ClassType.OwnFieldNames`）：
第 i 个子模式匹配第 i 个声明字段。约束：类型必须是 **record**；子模式数必须等于字段数。用户不写任何
解构方法。

```z42
[Record] class Point(int X, int Y);
// Point(a, b) → a ← X, b ← Y（声明序）
// Point(0, y) → 先测 X==0，再绑 y ← Y
```

> A1 位置解构限 **record class**（引用类型）；struct record 的位置解构（字节偏移读取）为后续特性。

## 实现原理

模式横跨编译器三层，每层一族模式节点，用一条递归下降 lowering 收口到既有 IR。

```mermaid
flowchart LR
    S["源码<br/>case Point(0, y) if y>0:"] --> P["PatternParser<br/>→ Pattern AST"]
    P --> B["PatternBinder<br/>→ BoundPattern<br/>(类型解析·歧义消解·<br/>record/arity 校验·<br/>绑定注册 TypeEnv)"]
    B --> E["PatternEmitter<br/>→ 既有 IR<br/>(IsInstance/Eq/<br/>FieldGet/BrCond)"]
```

| 层 | 文件（NEW） | 职责 |
|----|------------|------|
| AST | `z42c.syntax/Pattern.z42` + `PatternParser.z42` | 模式节点族 + 名字形状分流解析 |
| Bound | `z42c.semantics/BoundPattern.z42` + `PatternBinder.z42` | 绑定后模式树（携 resolved 类型/字段索引/绑定名）+ 类型解析·歧义消解·校验·绑定注册 |
| Emit | `z42c.semantics/PatternEmitter.z42` | 递归 test+bind lowering，短路 `BrCond` |

**lowering（`PatternEmitter.EmitMatch(subj, pat, matchL, failL)`）** 递归下降，匹配成功（绑定完成）跳
`matchL`、失败跳 `failL`：

| 模式 | test | bind |
|------|------|------|
| 通配 / 裸绑定 | 恒真 | 裸绑定：`name ← subj` |
| 常量 | `Eq(subj, value)` | — |
| 类型 `T` / `T x` | `IsInstance(subj, T)` | `T x`：命中后 `x ← as_cast(subj, T)` |
| 位置 `T(p_i)` | `IsInstance(subj, T)` ∧ 逐字段 | `field_get subj.f_i` → 递归子模式 |
| 属性 `T{F:p}` | `IsInstance(subj, T)`（T 可省）∧ 按名 | `field_get subj.F` → 递归子模式 |

三位点的**外壳**编排 match→guard→body/result：`switch` 是 case 链（失败落下一 case），`is` 收口成
布尔结果寄存器（绑定在 match 路径写入，true 分支支配其使用）。

### 两条实现铁律

- **常量模式 byte-identical**：常量模式 lowering 严格复刻旧 `switch` 的 `Eq(subj, value)` + `BrCond` 链
  （指令序 + 寄存器分配顺序一字不差）。z42c 自身源码在自举**不动点**（gen1 == gen2）中，其常量比较
  发码必须与旧编译器逐字节相同——否则自举断链。老 `x is T` / `x is T v` 同理走完全未改动的 `IsExpr`
  路径。
- **位置/属性字段直读**：字段读用 **`field_get subj.<name>` 直接从 subject 寄存器读**，**绝不**
  `as_cast(subj)→field_get`——后者被 JIT 误编（record 值语义合成曾踩此坑）。类型模式的绑定 `as_cast`
  仅存入局部、不接 `field_get`，安全。

### 无格式变更

纯编译期 lowering 到既有 IR（`IsInstance` / `Eq` / `FieldGet` / `BrCond` / `ConstBool`），**无 zbc/zpkg
格式 bump、无新 runtime、无新关键字**（扩 `switch`；`is` 已有；守卫复用 `if`）。新语法只在测试文件出现，
上一 nightly 的 z42c 仍能编当前源（满足两-nightly 纪律）。

## Deferred（后续独立特性）

- or-模式 `|` / `@` 绑定 / `..=` 范围 / 关系模式 `> 0`（A2）
- 解构声明 `Point(x, y) = p`（B，需元组式）
- 穷尽性诊断（C，封闭域 warning，复用 analyzer 框架）
- `with` 表达式 / `init`-only 访问器（D/E）
- struct record 位置解构、泛型 record 位置解构、元组模式
