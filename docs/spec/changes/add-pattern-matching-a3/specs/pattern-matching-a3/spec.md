# Spec: 模式匹配 A3 —— or-模式带绑定

> 归属：语言规范 · 模式匹配（A2 spec 之增量）。A3 放宽 A2 的「or 各 alt 不得绑定」限制。

## 文法

**无文法改动**——A3 复用 A2 的 `OrPattern := PrimaryPattern ( '|' PrimaryPattern )*`。差异纯在语义层
（允许各 alt 引入绑定 + 一致性约束）。

## 语义（A3 增量）

### or-模式各 alt 可引入绑定（放宽 A2 约束）

- or-模式的各 alt **可以引入绑定**（裸绑定 / `T x` / `@` / 位置·属性内子绑定 / 嵌套绑定均可）。
- **一致性约束**：各 alt 必须绑定**完全相同的变量集**——
  - **同名**：每个 alt 绑定的变量名集合必须相等（否则报错「must bind the same set of variables」/
    「is not bound by every alternative」）。
  - **同类型**：同名变量在各 alt 的类型必须**完全相同**（按类型名相等；不做 LUB / 公共基类推断）。
    否则报错「inconsistent type across alternatives」。
- **匹配语义**：从左到右尝试各 alt，任一匹配即整体匹配；匹配成功的那个 alt 的绑定值即为整体绑定值
  （合流）。绑定在守卫 / arm body 可见。
- 典型：
  ```
  case Circle(r) | Square(r) => use(r)              // 多变体共享字段
  case Pair(a, b) | Duo(a, b) => a * 100 + b         // 多绑定
  case Circle(r) | Square(r) if r > 10 => "big"      // 带守卫
  case v @ > 0 | v @ < 0 => v                        // @ + or
  case Box(Circle(r) | Square(r)) => r               // 嵌套 or（or 作子模式）
  ```

### 应用位点

同 A2：`switch` 语句 case / `switch` 表达式 arm 收 or（含带绑定）；`is` 表达式**不收** or（与 A2 一致，
`|` 在 is 保持位或语义）。

## 降级（lowering，无格式 bump）

无新 IR 指令、无 zbc/zpkg 变更。带绑定 or 用 **phi-free 合流**（稳定寄存器 + `Copy`）：

- 为每个统一绑定预分配一个稳定寄存器 `stable[k]`。
- 每个 alt 匹配成功 → 落地块把该 alt 绑的变量 `Copy` 进 `stable[k]` → 跳 matchL。
- matchL 处：绑定名 → `stable[k]`（各 alt 已搬入同一寄存器，单一一致）。
- **无绑定 or**（A2 全部用法）走**逐字未改**的旧 lowering（byte-identical）。
- **递归可组合**：嵌套 or 先合流成自己的稳定寄存器，外层读到单一寄存器再 Copy。

## 错误诊断

| 情形 | 诊断 |
|------|------|
| 各 alt 绑定数不同 | `or-pattern alternatives must bind the same set of variables` |
| 某 alt 缺某绑定名 | `or-pattern binding '<name>' is not bound by every alternative` |
| 同名不同类型 | `or-pattern binding '<name>' has inconsistent type across alternatives: <T1> vs <T2>` |

## 不做（Out）

- 跨类型绑定的 LUB / 公共基类推断（A3 要求类型完全相同）。
- `is` 中的 or / `@`。
- 解构声明（B）、穷尽性（C）等仍属后续迭代。
