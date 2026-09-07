# Tasks: 接口成员齐备性校验

> 状态：🟢 完成（已归档）| 创建：2026-09-07 | 归档：2026-09-07
> 范围裁决（2026-09-07，User）：**先实现全集并量欠债**（缺成员 + 签名不匹）；
> `ImportedSymbolLoader` 的 `IsStatic` 保真度**本轮顺手修**。

## 进度

- [x] 1. 探针坐实现状（缺成员 / `Self` 签名不符 双双静默通过）
- [x] 2. `InheritanceResolver` pass ⑥ `_checkIfaceMembersComplete` + `_substForIface`
- [x] 3. 顺手修 `ImportedSymbolLoader:266` 的 `isStatic` 硬编码 `false`
- [x] 4. 7 条单测（4 真门 + 3 无误报守卫）
- [x] 5. **量欠债 = 0**（stdlib 25 库 + compiler 全部）
- [x] 6. **两道对照**（阳性对照 + 真实构建面破坏性对照）
- [x] 7. **退回对照**：4 真门全红、3 守卫全绿
- [x] 8. 完整 GREEN（0 failed）+ 不动点 3/3 + stdlib-jit 331/331 + cross-zpkg-jit 20/20 + bootstrap 无越界
- [x] 9. 文档同步（generic-constraints.md 改写「写错不会被抓」那句 + InheritanceResolver 抬头注释）+ 归档

## 🎯 欠债 = 0（关键结论）

| 面 | E0412 |
|---|---|
| `build stdlib`（25 库，含 String / 12 基元 / collections） | **0** |
| `build compiler`（z42c 全部子系统） | **0** |
| `xtask test compiler` | **0 失败** |

⇒ 开门无需任何清理。这不是运气：`int` ≡ `Int32`（`CanonName()` 同为 `i32`）让 12 个基元天然
匹配；`OverloadsOf` 让 `String` 的双 `Equals` 不误判；AST 重解析让 `Bag<U> : IColl<U>` 不误判。
这三处**任何一处写错都会立刻炸出几十条假红**——零违反正是它们都对的证据。

## 🔒 三道对照（缺一不可，都已实测）

**① 阳性对照**（证明「零违反 ≠ 通道没通」）：

```
E0412: `Empty` implements `I` but does not define member `N`
E0412: `Empty` implements `I` but does not define member `M`
E0412: `WrongSig` implements `I` but no overload of `N` matches …(want `N$1$string`)
E0412: `Q` implements `IEq` but no overload of `Same` matches …(want `Same$1$Q`)   ← Self:=Q 生效
```
同文件里的 `Ok` / `P`（正确实现）**不报** —— 无误报。

**② 真实构建面破坏性对照**（证明校验活在真实路径上、不只在单测里）：
把 `Boolean.Equals(bool)` 临时改成 `Equals(int)` → `build stdlib` **失败**并报
``Boolean` implements `IEquatable` but no overload of `Equals` matches …(want `Equals$1$bool`)``；
改回即绿。

**③ 退回对照**（证明单测是真门）：注释掉 pass ⑥ 调用点后

| 用例 | 退回后 |
|---|---|
| `test_missing_interface_member_is_reported` | 🔴 |
| `test_wrong_signature_for_interface_member_is_reported` | 🔴 |
| `test_self_typed_interface_member_must_match_implementing_type` | 🔴 |
| `test_parent_interface_member_must_also_be_implemented` | 🔴 |
| `test_ok_self_typed_…_is_not_reported` | 🟢 两态同绿（无误报守卫） |
| `test_ok_generic_interface_type_arg_substituted_…` | 🟢 两态同绿（无误报守卫） |
| `test_inherited_implementation_satisfies_interface` | 🟢 两态同绿（无误报守卫） |

## 边界（本轮不做，已在 proposal 写明理由）

- **static-vs-instance 种类校验**（同名 static 但无 `override`）：`MethodSymbol` 无 `IsAbstract` 槽，
  判据要先想清楚。修好的 `IsStatic` 保真度是其前置。
- **`impl Trait for Target` 块补充的接口**：`_passImpls` 在本 pass **之后**跑，此刻还不在
  `InterfaceNames` 里。不是回归（此前一条都不查）。
