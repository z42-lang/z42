# Design: 值类型 + Type 对象 Object 方法 / 数组全路径名

## Architecture

```
a.GetType()  (a: struct/enum) ──┐
E.Red.GetType()                 ├─► CallEmitter: 值类型 receiver + method∈Object4?
                                │      ├─ GetType        → 折叠 Typeof(静态类型)   [无装箱]
a.ToString()/Equals()/GetHash() ┘      └─ ToString/Equals/GetHashCode（struct 未自声明）
                                             → __box_struct(recv) + VCall → runtime 协议
                                               (exec_vcall.rs:200-224，已存在)

typeof(X).GetType() (recv: Std.Type) ─► ③ 根因修复 → typeof(Std.Type)

typeof(int[]) ─► runtime build_type_ex：FullName={elemFullName}[]、Name={elemName}[]
```

## Decisions

### Decision 1: GetType on 值类型 → 编译期折叠 typeof（不装箱）

**问题：** struct/enum 的 `GetType()` 现崩（struct）/ 返错类型（enum i64→Std.Int32）。

**决定：** CallEmitter 识别「receiver 静态类型是值类型（struct/enum）且 method==GetType 且 0 实参」→ 直接发
`Typeof(静态类型FQN)`（等价 `typeof(T)`）。**理由**：值类型 sealed、无多态 → 编译期静态类型 == 运行期类型，
GetType 结果编译期已知。这与 change A 的 `typeof(int) ≡ 5.GetType()` 同源，且无装箱开销。C# JIT 亦如此优化。

- struct A → `typeof(A)` → 真句柄、FullName `Demo.A`、成员可枚举。
- enum E → `typeof(E)` → FullName `Demo.E`、`IsEnum=true`。

### Decision 2: struct 的 ToString/Equals/GetHashCode → 装箱 + VCall（复用 runtime 协议）

**问题：** struct 走静态 `Call {Struct}.{method}`（CallEmitter:103），Object 继承方法无函数体 → 崩。

**决定：** CallEmitter 的 struct-实例调用路径，当被调方法 ∈ {ToString, Equals, GetHashCode} 且 **struct 未自声明
该方法**（record 合成 / 用户声明的仍走其自身静态 Call）时：`__box_struct(recv)` 装箱 → 发 **VCall**（bare 方法名）。
runtime 装箱-struct Object 协议（[exec_vcall.rs:200-224](../../../../src/runtime/src/interp/exec_vcall.rs)）已处理：
GetHashCode→`__struct_hash_code`、ToString→短类型名（非 record）/ 合成（record）、Equals→解析方法。

**为何不改 runtime**：协议已完整，缺的只是编译器把 struct receiver 以 BoxedStruct 形式送达 VCall。

### Decision 3: ClassExtractor 的 struct-排除——保留元数据排除，仅在 CallEmitter 路由

**问题：** [ClassExtractor.z42:133](../../../../src/compiler/z42c.semantics/src/ClassExtractor.z42) `if (!isStruct)` 排除
struct 的 Object 四方法（注释「镜像 C# ExcludeFromImplicitObject」——**理解有误**，C# struct 经 ValueType 有这些方法）。

**选项：**
- A（保留排除 + CallEmitter 路由）：不动 struct 方法表元数据（不改 zbc TYPE section、不扰 self-host 字节），
  只在 CallEmitter 把调用路由对。绑定已能解析（bare 名，MemberCollector:191）。
- B（解除排除）：struct 方法表注入 Object 四方法 → `typeof(struct).GetMethods()` 含它们（更全 C# 反射面），
  但改 zbc 元数据 + self-host 字节 + 可能扰 struct 无 vtable 假设。

**决定：** 选 **A**。本次目标是「值类型能正确**调用** Object 方法」——CallEmitter 路由即根治该行为。
`typeof(struct).GetMethods()` 是否列 Object 方法属**反射枚举完备性**（另一维度），blast radius（格式/字节）大、
收益边际，列 Out-of-Scope 的后续。注释的「误解」在本变更订正（改注释 + 指向本 design）。

> 若实施中发现绑定层对 struct Object 方法解析不出（E0401）→ 回到选项 B 重新评估（届时停下问 User）。

### Decision 4: ③ Type 对象 GetType 返 null —— 根因 = 运行期 static/instance vtable 撞车（实施期钉准）

**问题：** `typeof(X).GetType()` 对 `Std.Type` receiver 返 null（本地/imported 类 typeof 皆然），链式 `.FullName`
→ `FieldGet on Null`。

**钉准结论（订正原猜测）：** 加诊断（`OverloadBinder._resolveOverload` 打印候选）证实**编译器重载决议本身正确**
——arity-0 `GetType` 与 static `GetType$1$string` 都在候选集，byArity 正确选中 arity-0 那个。**根因在运行期
vtable 构建**（`metadata/loader/type_registry.rs::merge_with_base`）：`Std.Type` 的**静态** `GetType(string)`
（`[Native("__type_get_type")]`）其限定名进 `own_methods`，建 instance vtable 时 `derive_simple_method_name`
剥掉 `$1$string` mangle 得简单名 `GetType`，**覆盖了**从 `Object` 继承来的 instance `GetType` 槽 → VCall
命中静态 extern（receiver 当 fqn）→ null。原猜测「SymbolCollector Object-stub 派发」**证伪**。

**根治：static 方法不进 instance vtable。** `TypeDescCold.own_static_flags`（index 对齐 `own_methods`，采自
`Function.is_static`）；`merge_with_base` 跳过 static 项（它们只经 mangled 直呼 `Call` 派发，从不虚派发）。
`needs_fixup` 的 `expected_vtable_count` 投影**同步**跳过 static（否则实际 vtable 恒少于投影 → fixup 永不收敛）。
反射 `GetMethods()` 仍从完整 `own_methods` 枚举 static 方法，不受影响。这是一条**通用**修复——任何类的 static
方法简单名撞上继承 instance 方法都受益，`Std.Type` 只是首个真实触发者。

### Decision 5: 数组 FullName/Name 全路径

**决定：** runtime `build_type_ex`（数组臂）令 `__fullName = {elemFullName}[]`、`Name = {elemName}[]`：
- 元素名经 `make_type_from_name(element)` 解析出其 Type，读 `__fullName`/`Name`（含 change A 的 `int→Std.Int32`）。
- `typeof(int[]).FullName` → `Std.Int32[]`、Name → `Int32[]`；`int[][]` 递归 → `Std.Int32[][]`。
- `GetElementType()` 仍读 `__elementName`（不变）。

## Implementation Notes

- **CallEmitter 值类型判定**：receiver 的 BoundExpr 静态类型是否 struct/enum（`Z42Type` 谓词；参照 struct 路径
  103 现有的 blob-struct 判定）。
- **折叠 GetType**：发 `TypeofInstr(静态类型 FQN, [])`（复用 typeof codegen，含 change A 的真句柄解析）。
- **装箱**：`__box_struct`（AccessEmitter:281 同款）→ 得 BoxedStruct 句柄 → `VCallInstr(dst, boxed, method, args, arity)`。
- **struct 自声明检测**：查 struct 的方法表是否含 `{method}$arity` / `{method}`（record ToString / 用户 Equals 存在时
  不走装箱协议、走其自身）。
- **数组元素递归**：`{elemFullName}[]` 中 elemFullName 对 `int[]` 元素是 `Std.Int32[]`（再递归）。

## Testing Strategy

- **Golden e2e**：
  - `value_type_object_methods.z42`：struct a 的 GetType(FullName=Demo.A)/ToString/GetHashCode/Equals；enum
    E.Red.GetType()==typeof(E)；`typeof(A).GetType().FullName==Std.Type`；对照 class 基线仍对。
  - `array_type_fullname.z42`：`typeof(int[]).FullName==Std.Int32[]`、Name==`Int32[]`、`int[][]`、用户类数组。
- **单元**：`reflection_tests.rs` 数组全路径名。
- **回归**：record 的 ToString/Equals（自声明路径不被本变更破坏）；`xtask test e2e --dir types` + 全量 `xtask test`。
- **确定性手验**：`xtask test e2e --file <probe> --no-build` 快速比对（本次探索已验证该配方）。
