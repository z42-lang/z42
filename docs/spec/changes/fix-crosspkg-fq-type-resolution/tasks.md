# Tasks: fix — 跨包全限定导入类型解析（反射对象类型可跨包成员访问）

> 状态：🟡 进行中 | 创建：2026-07-22 | 类型：fix（根因）
**变更说明：** `ImportedSymbolLoader._resolve` 对 FQ 书写的导入类型名做短名回退，解析为真 `Z42ClassType`。
**原因（根因）：** `ImportedSymbols.Classes` 以**短名**为键；`_resolve` 按签名原名查，遇跨命名空间、全限定书写的类型名（如 `Type.GetFields()` 返回的 `Std.Reflection.FieldInfo[]` 元素 `Std.Reflection.FieldInfo`）→ 短名键 miss → 降级 `Z42PrimType` 哨兵 → 消费包对 `Std.Reflection` 反射对象类型（FieldInfo/PropertyInfo/MethodInfo）成员访问报 `E0402 "member access on non-class"`。这让**任何 ≠ z42.core 的包都无法真正消费反射**（z42.test 只能靠 Type+Invoke-by-name 绕开）。
**文档影响：** compiler-z42c.md 或 book 编译器页（跨包类型解析）——归档前补。

## 变更点
- [x] 1.1 `ImportedSymbolLoader._resolve`：FQ miss 后按 `_shortName` 回退查 Classes（泛型实例化串/prim 不受影响：shortN==name 跳过）
- [ ] 1.2 验证（跨包反射成员访问）：serde 绑定器（z42.json，本 PR 同带）compile + run 通过 = 该修复的活体测试
- [ ] 1.3 自举字节 golden：更多导入类型从 prim 升 Z42ClassType → fixture 可能重生（CI 兜；不动点 gen1==gen2 靠确定性仍成立）
- [ ] 1.4 文档同步 + 归档

## 备注
- 本地不可编（stale 种子）→ 验证靠 CI。compiler 子系统并发争用 → 走隔离分支 + PR。
- 修在产出端（类型解析），非消费端打补丁——符合 philosophy 根因修复。
- 只处理 Classes（反射对象类型是 class）；接口 FQ 书写同类问题若出现另列。
