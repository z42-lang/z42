# Proposal：自由函数取引用的 target-typed 重载消解（零格式 bump）

> 类型：**lang**（新类型系统规则：方法组值的目标定向重载消解）
> 创建：2026-09-22 | 前序：[[free-function-overloads-program]]（#731 support / #739 use）

## Why

自由函数重载（#731/#739）落地后留下一处 D5 遗留：**把一个重载自由函数当值取引用**
时，编译器一律报 **E0425**，无论上下文有没有目标委托类型。

```z42
int  Parse(string s);
long Parse(string s, int radix);          // 同名重载（#731 起合法）

Func<string, int> f = Parse;              // 今天：E0425「cannot take a reference to
                                          //        overloaded free function `Parse`」
                                          // 期望：按目标委托 Func<string,int> 精确选中 int Parse(string)
```

现状根因（[ExprTyper.z42:100-114](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42)）：
裸标识符当值走 `_bindIdent`，该路径**无 target 参数**——拿不到赋值左侧的委托签名，
故多重载时只能保守报错。注释已明确标注这是 v1 的已知遗留：

> free-function-overloads v1：方法组取引用只在**无歧义**（该名恰一份）时可行。多重载取引用
> 需按目标委托类型定向消解（method-group target-typed resolution），v1 不做 → 报诊断。

这不是设计禁令，而是**目标类型没有接线进来**。z42 已有成熟的 target-typed 通道
（target-typed `new` / lambda 赋委托 / 集合字面量），本变更把自由函数取引用接入同一通道。

### 为什么值得做

- **完成度**：委托的头号用途就是把一个具名函数当回调传出去
  （`xs.ForEach(Print)` / `bus.Subscribe(Handler)`）。重载一旦可用，取引用不可用 =
  能力半残——用户被迫先赋给中间变量再传，或给函数改名避重载。
- **对称**：`methodof(Api.Parse(string))`（反射侧，#568）已能用**显式参数类型列表**在重载间
  精确选中；委托侧的取引用却做不到同一件事，是缺口。本变更让委托侧用**目标委托签名**
  自动完成同一消解——用户连参数类型列表都不用写。

## What（范围）

**仅自由函数**取引用的 target-typed 消解。四类「有目标委托类型」位置 + 调用实参位：

| 位置 | 例 | 通道 |
|---|---|---|
| 赋值 | `f = Parse;` | `AssignTyper` → `BindWithTarget`（已在） |
| 变量声明 | `Func<string,int> f = Parse;` | `StmtBinder`（需接线） |
| return | `return Parse;` | `StmtBinder`（需接线） |
| 字段/属性初始化 | `Func<...> F = Parse;` | `AssignTyper.BindInitValue` → `BindWithTarget`（已在） |
| 调用实参 | `xs.ForEach(Parse)` | 延迟位 → `BindArgsToSignature` → `BindWithTarget`（需扩延迟谓词） |

**消解语义 = 精确全签名匹配**：z42 委托相容 `Z42FuncType.IsAssignableTo`
（[Z42Type.z42:509](../../../../src/compiler/z42c.semantics/src/Z42Type.z42)）是**逐位精确相等、无协变/逆变**。
故合法目标只有签名（形参 + 返回）与委托**精确相等**的那个重载。⇒ 消解 = 用委托签名
精确过滤候选。**不用** `OverloadResolver.Resolve`（它按「可隐式转 + 最具体」，会选中随后又被
精确相容检查拒绝的候选，制造迷惑）。

## 明确不做（本变更范围外）

| 项 | 理由 / 去向 |
|---|---|
| **实例/静态方法组 `obj.M` / `T.M` 取引用的 target-typed 消解** | 走**不同派发路径**：合成 thunk 内 `VCall(裸名, arity)` 虚派发（[CallEmitter.z42:490](../../../../src/compiler/z42c.semantics/src/CallEmitter.z42)），要定向到非-primary 同-arity 重载大概率触及 **VM vtable 派发**；且现状是**静默选 primary**（无 E0425），并入还要决定无目标时是否改报错。**另开 change 跟踪**（2026-09-22 User 裁决拆分） |
| **变型（协变/逆变）委托转换** | z42 委托无变型（delegates-events.md §10），精确匹配即全部合法目标 |
| **无目标位的重载取引用**（`var f = Parse;` / 表达式语句） | 无委托签名可依据 → 保持 E0425（消息更新为提示「标注目标委托类型」） |

## 零格式 bump / 零 VM 改动

- 发射仍是既有 `LoadFnInstr @<qualified 名>`——只是把限定名从 base `FuncName` 改成选中重载的
  **`RegKey`**（`MethodSymbol.RegKey`，注册键单一真相）。#731 已让非-primary 自由函数重载
  按 `QualOf(ns, RegKey)` 注册进运行期 `func_index` 表（同 Call 消费的表）⇒ LoadFn 按同键
  必可解析，**零新 opcode、零 IR/zbc 格式改动**。
- **存量字节不变**：唯一/primary 自由函数 RegKey == base 名（#414 primary-bare），既有单份
  取引用发射逐字节不变 ⇒ 不打断自举不动点、无需两代自举。由**不动点 3/3 gen1==gen2** 证。

## 阻塞式前置验证（写实现前必须做完，见 tasks ①）

1. **LoadFn @非-primary RegKey 运行期可解析**（最高优先，整个「零 VM 改动」的支点）
2. **跨包非-primary 取引用**（imported 重载）端到端
3. **存量字节稳定**（不动点 3/3）
