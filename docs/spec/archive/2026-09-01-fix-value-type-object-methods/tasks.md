# Tasks: 值类型 + Type 对象 Object 方法 / 数组全路径名

> 状态：🟢 完成并归档（GREEN + self-host gen1==gen2）| 创建：2026-09-01 | 类型：vm/lang（编译器派发 + 运行期 vtable/反射）

## 进度概览
- [x] 阶段 0: ③ Type 对象 GetType null 根因定位（钉准 = 运行期 static/instance vtable 撞车，非编译器重载）
- [x] 阶段 1: struct/enum GetType 折叠 typeof
- [x] 阶段 2: struct ToString/Equals/GetHashCode 装箱 + VCall
- [x] 阶段 3: ③ 修复 + ④ 数组全路径名
- [x] 阶段 4: 测试
- [x] 阶段 5: 文档同步
- [ ] 阶段 6: 验证 GREEN

## 阶段 0: ③ 根因定位（先钉准再改）
- [x] 0.1 诊断定位：编译器**已正确**选中 arity-0 `GetType`（`OverloadBinder._resolveOverload` DBG 证实候选
      含 `GetType`(pc0) + `GetType$1$string`(pc1)，byArity 选前者）。null 来自**运行期 vtable**：静态
      `Type.GetType(string)` 简单名 `GetType` 撞车覆盖继承的 `Object.GetType` 槽 → VCall 命中静态 extern
      `__type_get_type`(receiver 当 fqn) → null。
- [x] 0.2 修复点定案 = 运行期 `type_registry.rs::merge_with_base`（static 不进 instance vtable），非
      SymbolCollector/CallEmitter。记入 design 决策 4（订正）。

## 阶段 1: struct/enum GetType 折叠 typeof
- [x] 1.1 `CallEmitter._emitCall` instance 分支入口：值类型 receiver + `GetType`0 参 → `_emitValueTypeGetType`
      发 `TypeofInstr(静态类型FQN)`。判据 `_isStructOrEnumStatic`（用户 struct（IsStruct && !IsScalarValue）
      或 EnumTypes 名）。
- [x] 1.1b **enum 字面量 `E.Red`**（静态类型 long）：`MemberResolver` 绑 `BoundLitInt` 时打 `EnumTypeName`
      origin 标记（`BoundExpr.z42`），`CallEmitter` 据标记折叠回 `typeof(E)`。保 z42 enum-as-int 模型不变、
      不扰自举字节。**（scope 追加 MemberResolver.z42 + BoundExpr.z42——design 未覆盖 `E.Red` 静态类型是 long
      这一事实，实施期发现并按最小改动解决。）**
- [x] 1.2 验证：struct A / enum E（字面量 + 变量）GetType FullName/Name/IsEnum。

## 阶段 2: struct ToString/Equals/GetHashCode 装箱 + VCall
- [x] 2.1 `CallEmitter` blob-struct 分支：method∈{ToString,Equals,GetHashCode} 且 `!ChainHasMethod`
      → `__box_struct` + VCall（`_emitBoxedStructObjectCall`）。
- [x] 2.2 record 合成 / 用户自声明（ChainHasMethod 命中）走自身静态 Call——回归确认（record ToString
      `R { A = 1, B = 2 }`、值 Equals；用户 `ToString` 不被短名拦截）。
- [x] 2.3 ClassExtractor 订正「ExcludeFromImplicitObject 误解」注释，指向 design 决策 3（选项 A：导出表仍
      不注入 Object 四方法，只 CallEmitter 路由）。

## 阶段 3: ③ 修复 + ④ 数组全路径
- [x] 3.1 ③ 根治：`TypeDescCold.own_static_flags`（index 对齐 own_methods，采自 `Function.is_static`）；
      `merge_with_base` + `needs_fixup` 两处**同步**跳过 static 项（不进 instance vtable / 投影一致，否则
      loader 不收敛）。
- [x] 3.2 ④ runtime `type_object.rs::make_type_from_name` 数组臂：递归解析元素 Type，`FullName={elemFull}[]`、
      `Name={elemName}[]`（空元素保 `Std.Array`）。`typeof(T[])` 与 `arr.GetType()` 同路一致。
- [x] 3.3 **（scope 追加）** `MemberResolver` 补 `Z42ArrayType` receiver 的 Object 方法分支 → 查 Object 取真实
      返回类型（`GetType`→`Std.Type`），修 `xs.GetType().FullName` 松绑 Unknown → 链式退化 FieldGet → null
      的**既有**缺陷（此前数组反射只经 `.__fullName` 字段读，未走属性链）。

## 阶段 4: 测试
- [x] 4.1 `src/tests/types/value_type_object_methods.z42`：struct 四方法 + record/用户 struct + enum GetType
      + Type 对象 GetType + class/基元基线。
- [x] 4.2 `src/tests/types/array_type_fullname.z42`：int[]/int[][]/string[]/用户类数组 FullName+Name；
      运行期 GetType 一致；GetElementType 不变。
- [x] 4.3 更新既有 `array_get_type.z42`（`Std.Array`→`Std.Int32[]`）、`object_get_type.z42`（数组上行）。
      （reflection_tests.rs 单测不可行：数组名合成需 z42.core 句柄解析，goldens 覆盖——与该文件 line-335
      既有说明一致。）
- [x] 4.4 回归：record ToString/Equals（value_type_object_methods 内覆盖）。

## 阶段 5: 文档同步
- [x] 5.1 `docs/book/src/runtime/struct-value-semantics.md`：新增「编译器派发：值类型 receiver 的 Object 方法」
      （GetType 折叠 + 装箱 VCall 路由 + enum origin 标记 + decision 3 订正）。
- [x] 5.2 `docs/book/src/runtime/reflection-type-identity.md`（新页 + SUMMARY 接线）：③ static/instance vtable
      撞车根因与 own_static_flags 修复；④ 数组全路径名递归 + 数组 GetType 返回类型。（按 doc-system D2：反射
      新知识写入 book，不回写已冻结的 docs/design/language/reflection.md。）

## 阶段 6: 验证 GREEN
- [x] 6.1 `cargo test --lib`（runtime metadata/vtable 改动）—— 21/21 通过
- [x] 6.2 `xtask test e2e --dir types` —— 122 passed 0 failed（新增 2 + 更新 2 全绿）
- [x] 6.3 `xtask test` 完整 gate —— ✅ GREEN all stages；self-host 不动点 3/3 pkg gen1==gen2
- [x] 6.4 spec scenarios 逐条覆盖（综合探针 ALL-SCENARIOS-OK 已验）

## 备注 / 实施期决策
- **决策 3 未转 B**：绑定层能解析 struct Object 方法（无 E0401），保留导出表排除（选项 A），仅 CallEmitter 路由。
- **enum `IsValueType` Out of Scope**：`typeof(enum).IsValueType` 现为 false（enum 元数据无 value-type 标志，
      pre-existing）；本次 enum 只修 GetType（对齐 proposal Out-of-Scope）。如需对齐 C#（enum IsValueType=true）
      另立（元数据/class_flags 维度）。
- **enum 的 ToString/Equals/GetHashCode 不在本次**（Out of Scope）。
- **`var dir = Enum.Member` 的 GetType**：z42 enum-as-int → `dir` 静态类型 long → `dir.GetType()`=Std.Int64
      （与 z42 enum-as-int 模型一致，非本次目标；`E.Red.GetType()` 字面量与显式 `E e` 变量两式已修）。
