# Proposal: `Self` 返回类型替换 —— 经接口调用不再漏出裸型参

> 类型：lang（类型推导语义）｜ 创建：2026-09-07
> 前置：`add-associated-types` PR-2 落 `Self`（#506）、`apply-self-to-core-protocols` 落 use（#525）

## Why

`Self` 交付时登记了一条 Deferred `self-return-type-substitution`
（[generic-constraints.md](../../../book/src/language/generic-constraints.md) 「实现模型」节）：
经**接口静态类型**调用一个返回 `Self` 的方法，结果类型是**型参 `Self` 本身**，而不是可用的类型。

实测确认（改动前，`--dump-bound`）：

```
interface IClone { Self Copy(); }
IClone c = new Point();
var x = c.Copy();
→ (decl x :Self (call-inst (ident c :IClone) Copy :Self))     ← `Self` 裸奔到调用方
```

`Self` 在调用方作用域里**没有任何意义**——它是接口内部的隐式型参。拿到它的变量既不能当具体类型用，
也不能当接口用，等于这一步的类型信息整个丢失。

## What Changes

`MemberResolver` 新增 `_substSelf(t, selfT)`（结构镜像既有 `_substGeneric`：数组 / 泛型实参递归），
在**接口收者**的两个返回点把 `Self` 替换成接收者的静态接口类型：

| 位置 | 场景 |
|---|---|
| `MemberResolver.z42:96` 接口收者方法调用 | `IClone c; c.Copy()` |
| `MemberResolver.z42:197` 接口收者属性 getter | `interface IBox { Self Inner { get; } }` |

改动后：

```
var x = c.Copy();  → (decl x :IClone (call-inst (ident c :IClone) Copy :IClone))
```

### 为什么替换成「接口本身」而不是别的

- **这是可靠上界**：任何实现方都实现了该接口，故把结果当接口用一定成立。
- **不是具体类型，也不该是**：经接口静态类型调用时，编译期根本不知道运行期具体类是谁。
  要具体类型就在**具体类**上调用——那条路走实现方签名，本次改动不碰
  （`Point p; p.Copy()` → `Point`，改动前后都对，已加对照用例守住）。
- Rust 的对应做法是干脆禁止（`-> Self` 不是 object-safe，`dyn Clone` 不可用）。z42 选**上界替换**
  而非禁止：z42 接口没有 object-safety 概念，禁止会平白砍掉一类可用写法，而上界替换是安全的。

## 不在本轮

- **`Self` 出现在形参位**（`interface IEq { bool Same(Self other); }`，经接口调用 `e.Same(x)`）：
  `BindArgsToSignature` 仍拿未替换的签名。今天不炸是因为**实参类型检查对型参形参放行**
  （`ConstraintChecker` 把裸 `Z42GenericParamType` 当「假定满足」，见 `:256`/`:267`/`:309`）。
  真要收紧属于「接口形参逆变性」那一档，与本轮的返回位（协变、有唯一安全上界）不是一回事。
- **接口成员齐备性校验**：拆为独立 change（`add-interface-satisfaction-check`），见其 proposal。

## 验证

- 🔒 **真门（已做退回对照实测）**：把两个 `_substSelf` 调用点退回后，
  `test_self_return_type_substituted_to_receiver_interface` 与
  `test_self_coexists_with_interface_own_type_param` 双双变红，报
  `expected …:IClone… but got …:Self…`；恢复后转绿。
- 对照面 `test_self_on_concrete_receiver_still_gives_concrete_type` **两态同绿**——它是控制组
  （具体类路径不经过 `_substSelf`），已在用例注释里写明它不是门。
- 完整 GREEN + 自举字节不动点。
