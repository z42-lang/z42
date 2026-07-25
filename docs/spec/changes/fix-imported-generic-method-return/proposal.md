# fix-imported-generic-method-return

> 子系统：`compiler`（z42c.semantics 类型解析）。变更分类：**fix**（轻量：IMPL → GREEN → COMMIT）。
> User 授权隔离 worktree 预抢 `compiler` 锁（持有者 `split-irgen-class`）；GREEN 以自举不动点 + CI 为权威。

## 背景

`add-z42-repl` 收官时 `z42.interactive` 的 `.vars`/`.usings` 调 `s.VarNames.ToArray()` 报
`E0402: return type T[] not assignable to string[]`。`VarNames` 是 `List<string>`，`List` 自
`z42.collections` **导入**。当时在 `ScriptState` 侧加 `UsingsArray/VarNamesArray`（Count+索引器手工
物化）**绕过**。本 change 根治该缺陷。

## 根因

`MemberResolver._bindInstanceMemberCall` 对 `Z42InstantiatedType` receiver（`List<string>`）解析
方法调用时，返回类型只判**裸 `T`** 并松绑 `Unknown`（旧 `if (itRet is Z42GenericParamType)`），
**既不按 receiver 的 `TypeArgs` 替换 `T→string`，也不处理 `T[]`**（数组元素是形参）。于是
`ToArray()` 的 `T[]` 原样返回 → 赋给 `string[]` 报 E0402。字段访问轴（`Z42InstantiatedType` 成员）
同样只松绑 `Unknown`。

`#30`（`fix-crosspkg-generic-import`）修的是**导入泛型字段的字面类型**（`Store.items : List<int>`
解析成 `Z42InstantiatedType`）；本 change 是其姊妹——**成员访问/方法返回轴**的类形参替换。

## 方案

1. `Z42ClassType` 加有序 `GenericParamNames` / `GenericParamCount`，两处构造回填：
   - 本地：`SymbolCollector._putClassStub`（`ClassDecl.TypeParams.Names`）。
   - 导入：`ImportedSymbolLoader`（`ExportedClassZ.TypeParams`，TSIG 已带形参名）。
2. `MemberResolver._substGeneric(t, inst)`：按 `inst.Def.GenericParamNames` ↔ `inst.TypeArgs` 索引
   对齐，**递归**替换——裸 `T`→arg、`T[]`→subst(T)[]、嵌套 `List<T>`→`List<subst(T)>`。形参名不在
   Def 形参表（方法级泛型形参 `U`）→ 松绑 `Unknown`（沿用旧 GS6，避免误绑）。
3. 方法返回（GS6 call 路径）+ 字段访问（GS6 member 路径）均改调 `_substGeneric`。

## GREEN 判据

- ✅ 复现修复：old z42c `E0402` → gen1 z42c 编过并运行正确（`List<string>.ToArray()`→2、"z42"）。
- ✅ 自举不动点：gen1 == gen2 **byte-identical**（semantics/pipeline/driver 三包）——改动确定、
  z42c 自身源在新旧逻辑下字节中性。
- ✅ 全量 stdlib 用 gen1 编译通过（无 miscompile）。stdlib 部分 zpkg 字节变化（泛型成员返回类型
  由 Unknown 变具体，dst tag 更精确、同尺寸）——预期且确定性；stdlib zpkg 是构建产物无 committed
  golden，不动点自洽。
- 回归测试：`src/tests/generics/imported_generic_method_return.z42`（方法返回 + 直接 `List<int>.ToArray()`）。
- 完整 gate（test compiler 单测 + e2e + test stdlib interp/jit）以 CI 为权威。

## 后续可考虑（本 change 不做）

- `ExprTyper` 里其它 `Z42GenericParamType → Unknown` 松绑点（静态/重载路径）是否也应替换——
  当前只改成员访问/方法返回两轴（实测足以修复 REPL 场景 + 覆盖 stdlib 泛型用法）。
- `ScriptState` 的 `UsingsArray/VarNamesArray` 绕过访问器可在本 fix 合入后移除（非必须，无害保留）。
