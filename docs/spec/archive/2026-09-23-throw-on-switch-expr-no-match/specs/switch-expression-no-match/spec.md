# Spec: `switch` 表达式无匹配臂的语义

## ADDED Requirements

### Requirement: 无任何臂匹配时抛 SwitchExpressionException

#### Scenario: 开放域（int）subject 落空
- **WHEN** `int n = 5; int a = n switch { 1 => 10, 2 => 20 };`
- **THEN** 求值该表达式时抛 `Std.SwitchExpressionException`
- **注**：此前 `a` 拿到**未初始化寄存器**——打印成 `null`，`a + 1` 打出垃圾值
  （实测 `1`；换程序形状曾打出 `17179869186`）。且 subject 是 `int` ⇒ **`W0700` 不覆盖，
  编译期一条诊断都没有**

#### Scenario: bool subject 落空
- **WHEN** `int n = 5; bool b = n switch { 1 => true };`
- **THEN** 抛 `Std.SwitchExpressionException`（此前 `b` 打成 `null`）

#### Scenario: string 结果类型落空
- **WHEN** `int n = 5; string s = n switch { 1 => "x" };`
- **THEN** 抛 `Std.SwitchExpressionException`（此前 `s == null`，要到 `s.Length` 才崩）

#### Scenario: enum subject 漏成员
- **WHEN** `enum Direction { North, East, South, West }`，
  `d switch { Direction.North => "N", Direction.East => "E", Direction.South => "S" }`
  在 `d == Direction.West` 时求值
- **THEN** 编译期照旧报 `W0700`（warning，不阻断），**运行期抛 `Std.SwitchExpressionException`**

#### Scenario: 守卫全部为假也算落空
- **WHEN** `int n = 5; int a = n switch { var x when x > 10 => 1 };`
- **THEN** 抛 `Std.SwitchExpressionException` —— 模式匹配了但守卫为假 ⇒ 该臂不采纳

### Requirement: 异常可被捕获，消息带落空的值

#### Scenario: catch (Exception) 抓得到
- **WHEN** 落空的 switch 表达式放在 `try { ... } catch (Exception e) { ... }` 里
- **THEN** 进入 catch 分支；`e.GetType().FullName == "Std.SwitchExpressionException"`
- **注**：这是与 `bail!` 内部错误的分水岭——内部错误 `catch (Exception)` **抓不到**

#### Scenario: 消息非空且含落空的值
- **WHEN** 捕获到该异常
- **THEN** `e.Message` 非空字符串，且含 subject 的字符串化值（如 `5`）
- **注**：消息由 emitter 合成 `ConstStr ++ ToStr(subject)` 后经 ctor 传入 ⇒ 这条同时钉住
  `ObjNew.CtorKnown` 在合成站点被正确置位（位没置上 ⇒ ctor 不被调 ⇒ `Message` 为空）

#### Scenario: 落空位置出现在栈回溯里
- **WHEN** 该异常未被捕获、传播到顶层
- **THEN** 栈回溯含抛出点所在函数与源码行
- **注**：位置**不进 `Message`**——`Throw` 终结符运行期已做 `resolve_line` +
  `populate_stack_trace`，位置免费；嵌进消息会把构建机路径烤进 zbc、破坏字节不动点

### Requirement: 有无条件兜底臂时行为与指令序列均不变

#### Scenario: `_ =>` 兜底
- **WHEN** `int a = n switch { 1 => 10, _ => 0 };`
- **THEN** `n == 5` 时 `a == 0`，不抛；**编译产物与本变更前 byte-identical**
- **理由**：兜底臂那一支提前结束循环（`ai = sw.ArmCount`）⇒ 落空点不可达 ⇒ 不发任何新指令

#### Scenario: 裸绑定兜底
- **WHEN** `int a = n switch { 1 => 10, var x => x };`
- **THEN** 同上，不抛，byte-identical

#### Scenario: 带守卫的臂不算兜底
- **WHEN** `int a = n switch { 1 => 10, var x when x > 3 => x };`
- **THEN** `n == 2` 时抛（守卫为假 ⇒ 无臂采纳）；`n == 5` 时 `a == 5`

### Requirement: switch 语句不受影响

#### Scenario: 语句形态无匹配 case
- **WHEN** `switch (n) { case 1: ... break; }`，`n == 5`
- **THEN** 什么都不做，**不抛**（C# 同）；指令序列与本变更前 byte-identical

### Requirement: z42c 与 stdlib 自身发码不变

#### Scenario: 产品代码零站点
- **WHEN** 用改后的 z42c 重建 z42c 自身与 stdlib
- **THEN** `CompilerFingerprint` 不变 —— `src/compiler` / `src/libraries` /
  `src/toolchain` / `scripts` 里 switch 表达式共 17 个、**无兜底臂者 0 个**
- **注**：`z42c.semantics/tests/exhaust/exhaust_tests.z42` 的 13 个不穷尽 switch 是
  **测试输入的源码字符串**（经 `SemanticDump.FirstErrorCode` 只做类型检查），从不发射、从不执行
