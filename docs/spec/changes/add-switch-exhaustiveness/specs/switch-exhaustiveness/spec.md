# Spec: switch 穷尽性诊断（模式匹配 C）

## 诊断

`switch`（语句 + 表达式）的 subject 属**封闭域**（bool 或 enum）、未覆盖全部情形、且无无条件兜底臂时，
在 subject 位置报 **`W0700`**（warning，默认开启，不阻断编译）。

## 封闭域

| 域 | 穷尽条件 |
|----|----------|
| **bool** | 常量臂覆盖 `true` ∧ `false`，或存在无条件兜底 |
| **enum** | 常量臂的整数值集 ⊇ enum 全成员整数值集，或存在无条件兜底 |

其它类型（int / string / 类 / ...）为**开放域**，不检查。**sealed 类层次 out-of-scope**（z42 sealed=final，
无封闭子类集语义，无反向子类索引）。

## 无条件兜底

以下任一臂使 switch 视为穷尽（不报）：

- `default` 臂（无 pattern）；
- 通配 `_` 或裸绑定 `x` 模式，**且无守卫**。

带守卫的臂（`pattern if guard`）**不计**入无条件覆盖（守卫可能为假）。

## 覆盖计算

- 常量模式 → 覆盖其常量值（enum 成员在绑定期降级为整数字面量，故 enum **按整数值**比对；别名成员
  同值天然合并）。
- or-模式 `A | B | C` → 递归展开，各 alt 计入覆盖。
- 范围 / 关系 / 类型 / 位置模式 → 不覆盖单一常量值（保守，可能过报）。

## 示例

```z42
enum Color { Red, Green, Blue }

// ⚠️ W0700：缺 Color.Blue
int a = c switch { Color.Red => 1, Color.Green => 2 };

// ✓ 全覆盖
int b = c switch { Color.Red => 1, Color.Green => 2, Color.Blue => 3 };

// ✓ or 覆盖全部
int d = c switch { Color.Red | Color.Green | Color.Blue => 0 };

// ✓ default 兜底
int e = c switch { Color.Red => 1, _ => 0 };

// ⚠️ W0700：bool 缺 false
int g = flag switch { true => 1 };

// ✓ int 是开放域，不检查
int h = n switch { 1 => 1, 2 => 2 };
```
