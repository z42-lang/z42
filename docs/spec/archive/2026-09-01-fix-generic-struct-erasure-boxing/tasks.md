# Tasks: 修复泛型（实例化）值 struct 的值相等 Equals

> 状态：🟢 已完成 | 创建：2026-09-01 | 完成：2026-09-01 | 类型：fix

**变更说明：** 泛型 record struct 的 `.Equals()` 对相等值误返 false。两个独立根因：
1. **本地用户泛型 record struct**（`GRec<int,int>`）：`BoxIfNeeded` 在擦除边界（→`object`）装箱值 struct 时判据只认 `Z42ClassType`，漏 `Z42InstantiatedType`（泛型实例化 struct）→ 实参裸 `StructRef` 传给合成 `Equals(object)` → runtime `is_instance` 无 StructRef 臂 → else 分支返 false。
2. **imported stdlib 泛型 record struct**（`ValueTuple2` 等）：跨包可见的合成 `Equals$1(object)` 与继承的 `Object.Equals(object)` **同签名** → `_collectOverloads` 按 RegKey 去重（`Equals$1`≠`Equals`）二者并存 → 重载决议歧义（伪 E0425）→ 解析失败 → 松绑不装箱 → 运行期同上误返 false。

**原因：** ① `if (vt is Z42ClassType)` 未 unwrap `Z42InstantiatedType.Def`；② 重载收集未实现 method-hiding（派生同签名方法应隐藏基类）。

**文档影响：** `docs/book/src/runtime/struct-value-semantics.md`（装箱边界 + 值相等派发机制节）。

## 根因定位（已实测逐指令钉准）
- 症状：`GRec<int,int>` / `(1,2)` 的 `.Equals` 值相等返 false；非泛型 record struct 正常。
- trace：合成 `Equals$1` 仅执行 `IsInstance(other,"…")=>false` + `ConstBool false`；`other` 到达时是裸 `StructRef`（非 `BoxedStruct`）。
- 根因①点：[TypeChecker.z42 `BoxIfNeeded`](../../../../src/compiler/z42c.semantics/src/TypeChecker.z42) `vt is Z42ClassType` 判据。
- 根因②点：[OverloadBinder.z42 `_collectOverloads`](../../../../src/compiler/z42c.semantics/src/OverloadBinder.z42) RegKey 去重 → `RESOLVE na=2 … ambiguous=Y`。

## 任务
- [x] 1.1 `TypeChecker.BoxIfNeeded`：unwrap `Z42InstantiatedType.Def` 判 IsStruct，装箱名用 `.Def.Name()`（擦除基名）。
- [x] 1.2 `OverloadBinder._collectOverloads`：按**有效签名**（简单名+形参规范类型）跨基链去重（method-hiding，派生隐藏基类同签名）+ `_overloadSigKey` 助手。
- [x] 1.3 回归测试：`src/tests/types/generic_struct_equals.z42`（本地泛型 record struct + ValueTuple；值元素 + string 引用元素；Equals 相等/不等 + GetHashCode 一致）。GREEN interp+jit ✅。
- [x] 1.4 文档同步：`docs/book/src/runtime/struct-value-semantics.md`（新增「泛型（实例化）值 struct 的值相等边界」节 + 页头对齐日期）。
- [x] 1.5 验证：完整 `xtask test` **全绿**（含 z42c self-host 不动点 3/3 gen1==gen2）。runtime 未改动 → 无需 cargo test。
- [ ] 1.6 归档 + PR。

## 备注
- 自举风险：改热路径 coercion + 重载收集；z42c 自建 + stdlib 25/25 已过；需完整 GREEN + gen1==gen2 收敛确认。
- method-hiding 只对「派生与基类同签名（含 $N mangle 差异）」生效；type-based 重载（不同形参类型）签名各异、不受影响；override（同 RegKey 同签名）结果不变。
