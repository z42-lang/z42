# Proposal: 模式匹配 A3 —— or-模式带绑定（各 alt 绑定集一致 + phi-free 合流）

## Why

模式匹配核心 A1（#306）建成三层递归下降引擎，A2（#308）补齐 or `|` / `@` / `..=` / 关系四组合子。
但 A2 对 or-模式加了一条**硬限制**：**各 alt 不得引入绑定**——`case Circle(r) | Square(r):` 会报
`or-pattern alternatives may not introduce bindings (A2)`。原因：不同 alt 把同名变量绑到**不同寄存器**
（Circle 的字段 vs Square 的字段），到 arm body 时该变量需要是两者的**合流**（phi），A2 把这块 defer 了。

这个限制挡住了 Rust 模式匹配最自然的用法之一——**「多个变体、同一处理」**：

```
// Rust：多个变体共享一个字段，合并处理
match shape {
    Circle(r) | Square(r) => use(r),   // r 来自任一变体
    ...
}
```

A3 补齐这一块：**or-模式的各 alt 可以引入绑定**，只要各 alt 绑定集（变量名 + 类型）**完全一致**。
这是**纯编译期 lowering**（无新语法、无新 token、无 IR 变化、无 zbc/zpkg 格式 bump）——parser 早在 A2 就已
解析出带绑定的 or-AST，A3 只是移除 binder 的报错、在 binder 加绑定集一致性校验、在 emitter 加合流 lowering。

## What Changes

在 A2 的 `BoundOrPattern` 上加**绑定元数据**（`BindNames` / `BindTypes` / `BindCount`），改写
`PatternBinder._bindOr`（绑定集收集 + 一致性校验 + 统一注册）与 `PatternEmitter` 的 or lowering
（phi-free 合流寄存器）。**仅 3 个 semantics 文件 + 1 测试 + 文档**，零语法 / 零格式改动。

| 维度 | A2（现状） | A3（本 change） |
|------|-----------|-----------------|
| or 各 alt 绑定 | ❌ 报错 | ✅ 允许，绑定集须一致 |
| 绑定集一致性 | — | 各 alt 绑**同名同类型**（User 裁决：类型完全相同，否则报错） |
| 嵌套 or 带绑定 | — | ✅ 支持（`Box(Circle(r) \| Square(r))`，递归可组合） |
| 合流机制 | — | **phi-free**：每绑定预分配稳定寄存器，各 alt 成功后 `Copy` 进稳定寄存器再跳 matchL |
| 应用位点 | switch-stmt / switch-expr | 同（`is` 仍不收 or，与 A2 一致） |

### 合流机制（phi-free，无需 SSA phi 节点）

z42 IR 无 phi 节点，绑定在 A1/A2 是**零成本别名**（`Locals.Put(name, reg)` 指向既有寄存器）。or 各 alt
产出不同寄存器 → 别名失效。A3 用 **稳定寄存器 + `CopyInstr`** 解决：

1. binder 校验各 alt 绑定集一致，算出统一绑定集 `{name: type}`。
2. emitter 为每个统一绑定**预分配一个稳定寄存器** `stable[k] = Alloc(ToIrType(type))`。
3. 每个 alt 匹配成功 → 落自己的 `okL` 块 → 把该 alt 绑的每个变量（此刻在 `Locals`）`Copy` 进 `stable[k]` →
   跳 matchL。
4. matchL 处：所有绑定 = 稳定寄存器（单一、一致），守卫 / arm body 正常读取。

**递归可组合**：嵌套 or 先合流成**自己的**稳定寄存器，外层 `Locals.Get(name)` 读到单一寄存器再 Copy —
无需特判嵌套深度。

## Scope（允许改动的文件）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.semantics/src/BoundPattern.z42` | MODIFY | `BoundOrPattern` +`BindNames`/`BindTypes`/`BindCount` |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | `_bindOr` 重写（子作用域收集 + 一致性校验 + 统一注册）；删死代码 `_patternBinds` |
| `src/compiler/z42c.semantics/src/PatternEmitter.z42` | MODIFY | or lowering 加 `BindCount>0` 合流分支（`BindCount==0` 保持 A2 byte-identical） |
| `src/tests/pattern-matching/pattern_a3.z42` | NEW | e2e：headline / 多绑定 / 守卫 / @+or / 嵌套 or；interp+jit 双验 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 补 A3 or 带绑定语法 + 合流机制 |
| `examples/patterns.z42` | MODIFY | 补 A3 示例（可选） |

**Out（后续 change）**：解构声明 `Point(x,y) = p`（B）；穷尽性诊断（C）；`with`（D）；`init`（E）；
元组（F）；`is` 中的 or / `@`；跨类型的绑定 LUB（A3 要求类型完全相同）。

## 自举 / 格式影响

- **无 zbc/zpkg 格式 bump、无新 runtime、无新 token / 语法**：`CopyInstr` 早已存在（IrInstr.z42）；or-带绑定的
  AST 早在 A2 由 parser 产出（A2 仅在 binder 报错拦截）。
- **两-nightly 纪律满足**：or-带绑定**只在 e2e 测试文件出现**，z42c / stdlib / xtask 源一律不用 → 上一 nightly
  的 z42c 仍能编当前源码。
- **自举字节不动点**：`BindCount==0`（无绑定 or，即 A2 的全部用法）走**完全不变**的旧 lowering 路径
  → gen1==gen2 天然成立。z42c 源无任何 or-模式 → 更无影响。
