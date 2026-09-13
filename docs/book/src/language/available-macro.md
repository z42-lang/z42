# `available!()` —— 符号可用性探测

> 用途：让代码在**依赖 zpkg 版本 skew**（编译时依赖 v2、运行时只加载到 v1）下仍能安全降级。
> 机制页（VM 侧折叠与剪枝）见 [加载期可用性折叠](../runtime/availability-folding.md)。
> **为什么需要它**：不加保护时，缺符号会在**用到那一刻**抛可 catch 的
> `Std.MissingSymbolException`（见[缺符号不再静默](../runtime/missing-symbol-resolution.md)）。
> `available!()` 是这条规则的**唯一显式豁免通道**——被保护的分支整块剪掉，其符号永不参与解析。

## 它解决什么

包 A 编译时依赖包 B v2（有 `B.Foo()`），部署时 `libs/` 里放的却是 B v1（没有 `Foo`）。
A 想写「有就用新的、没有就走老路」：

```z42
if (available!(NewApi.Feature)) {
    Console.WriteLine(NewApi.Feature().ToString());
} else {
    legacyPath();
}
```

`available!(X)` 求值为 `bool`：**X 在运行时实际加载到的依赖图里能否解析**。

**关键性质：同一份编译产物，在两种依赖图下走不同分支**——不需要为每种依赖组合各编一份。

## 语义：编译期 vs 运行期

| 时机 | 语义 |
|---|---|
| **编译期** | X **必须存在**，否则 `E0401`。这是拼写错误 / 重构失配的防线，与「运行期缺失」是两回事 |
| **加载期** | VM 判定 X 在当前加载图里能否解析 → 折成 `true` / `false` 常量 → **剪掉走不到的那条分支** |
| **运行期** | 零开销。分支早已消失，不是「每次判断一次」 |

被剪掉的分支里的调用点**永不参与符号解析**——这正是 `available!` 的核心价值：它是「缺符号
即报错」这条规则的**唯一显式豁免通道**。没有它，一个永远走不到的 `NewApi.Feature()` 也会
在函数准备期被校验到并抛出。

## 参数形态

只接受**符号引用**，两种：

```z42
available!(SomeType)          // 类型（class / interface / enum）
available!(SomeType.Method)   // 该类型上的静态或实例方法
```

不接受字面量、任意表达式、变量（`E1102`）。

### v1 限制：方法必须唯一

目标解析到多个重载 → `E1101`。签名消歧语法（如 `available!(F.Bar(int, string))`）**尚未实现**。

绕法：改探测类型（`available!(F)`），或等消歧语法落地。

> 为什么不放宽：版本 skew 的绝大多数场景是「整个 API 新增了」，不是「某个重载新增了」。
> 为不确定的需求先做签名语法不划算。

## 位置

`available!()` 是**表达式位**宏，可用于 `if` 条件、局部初始化等任何取值处。

**不能**用作参数默认值（`E0450`）——参数默认值必须是编译期常量，而 `available!` 的值到
加载期才确定。这与 `caller_member!()` 等 caller 族宏正好相反（那些只合法于参数默认值位）。

## 诊断

| 码 | 场景 |
|---|---|
| `E0401` | 目标在**编译期**不存在 |
| `E1101` | 目标解析到多个重载，无法唯一确定 |
| `E1102` | 参数不是符号引用，或参数个数不为 1 |
| `E0450` | 出现在参数默认值位（表达式位宏用错地方） |

## 与 `[Invariant]` 的能力差异

两者都做「常量折叠 + 死分支消除」，但**能力不同**，别混用：

| | 求值时机 | 效果 |
|---|---|---|
| `available!(X)` | **加载期**（只查符号表，不执行任何代码） | 原地 **CFG 剪枝**：死分支的指令**物理消失**，interp 与 JIT 都看不到 |
| `[Invariant]` native | **首次调用**（必须真的 call 进 native 代码） | 只能常量化条件；JIT 侧死块能被 DCE，但 bytecode 层面分支还在 |

要解决版本 skew 的「符号不存在」，**只能用 `available!`**。

## 示例

完整可运行示例见：

- `src/tests/cross-zpkg/available_present/` —— 依赖在场 → 走新路径
- `src/tests/cross-zpkg/available_skew/` —— 依赖运行期缺失 → 走旧路径（**同一份产物**）
- `src/tests/optimization/available_exception_table/` —— 在 `try/catch` 中使用（只折不剪，见机制页）
