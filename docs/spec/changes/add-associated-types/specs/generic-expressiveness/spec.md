# Spec: 泛型表达力 —— `Self` 与关联类型（PR-2 / PR-3）

> Capability：`generic-expressiveness`（新建）
> 长期规范落点：`docs/book/src/language/generic-constraints.md` + `docs/design/language/generics.md`

---

# PR-2：`Self` 类型（仅接口）

## ADDED Requirements

### Requirement: 接口内 `Self` 指代实现类型

`Self` 在接口声明内部是一个隐式型参，指代「实现该接口的具体类型」。语义上等价于今天的
F-bounded 模式 `interface I<T> where T : I<T>` 里的 `T`。

#### Scenario: 接口方法签名用 `Self`
- **WHEN** 声明 `interface INum { static abstract Self Add(Self a, Self b); }`
- **THEN** 编译通过；`Self` 解析成型参类型（与今天的 `T` 同款），不产生未定义类型诊断

#### Scenario: 实现类型接入
- **WHEN** `class Int32 : INum` 且 `Int32` 提供 `static override Int32 Add(Int32 a, Int32 b)`
- **THEN** 编译通过，运行期 VCall 派发命中该实现
- **注**：派发键不含类型（`static abstract` 走裸名键），`Self` 不影响键

#### Scenario: 约束位省略类型实参
- **WHEN** 写 `class Dict<K, V> where K : IEquatable`（不写 `<K>`）
- **THEN** 语义等价于今天的 `where K : IEquatable<K>`，校验行为完全一致
- **注**：约束模型本就归约成裸名 `"IEquatable"`，两种写法天然同义

#### Scenario: 跨包 `Self` 往返
- **WHEN** 包 A 声明 `interface INum { static abstract Self Add(Self, Self); }`，包 B `using A` 并实现它
- **THEN** 包 B 侧 `Self` 正确解析成型参类型，不落到 `Z42ClassType.Builtin("Self")` 兜底

#### Scenario: 跨包泛型接口方法的型参不再退化（顺带修的既有缺口）
- **WHEN** 包 A 声明 `interface IBox<T> { T Get(); }`，包 B 导入并使用
- **THEN** 方法签名里的 `T` 解析成 `Z42GenericParamType`，而非今天的 `Z42ClassType.Builtin("T")` 兜底

### Requirement: `Self` 的作用域限于接口

#### Scenario: 类里使用 `Self` 报错
- **WHEN** 在 `class Foo { Self Clone() { ... } }` 中使用 `Self`
- **THEN** 报 `E0443`（未定义类型），与其它未定义类型同一诊断，不新增错误码

#### Scenario: `Self` 不是保留字
- **WHEN** 已有代码把 `Self` 用作变量名 / 字段名 / 非接口上下文的标识符
- **THEN** 编译行为不变（`Self` 是上下文相关的类型名，不进 lexer 关键字表）

## Pipeline Steps（PR-2）

- [ ] Lexer —— 不涉及（`Self` 保持 `Identifier`）
- [ ] Parser / AST —— 不涉及（`_parseType` 已能产 `NamedType("Self")`）
- [x] TypeChecker —— `ResolveTypeP` 加 `Self` 分支；`TypeEnv` / `MemberCollector` 透传接口上下文
- [x] 元数据往返 —— `ClassExtractor` 导出 `"Self"` 字符串；`ImportedSymbolLoader._resolve` 解码
- [ ] IR Codegen —— 不涉及（`Self` 不进派发键）
- [ ] VM interp —— 不涉及

---

# PR-3：关联类型

## ADDED Requirements

### Requirement: 接口可声明关联类型

#### Scenario: 声明关联类型
- **WHEN** 写 `interface IIter { type Item; Item Next(); }`
- **THEN** 编译通过；`Item` 在该接口内可作类型名使用

#### Scenario: `type` 仍可作普通标识符
- **WHEN** 已有代码在类体内写 `int type = 3;` 或 `var type = f();`
- **THEN** 编译行为不变（`type` 是上下文关键字，按三 token 前瞻拦截，不进 lexer）

#### Scenario: 实现方显式绑定
- **WHEN** 写 `class IntIter : IIter { type Item = int; int Next() { ... } }`
- **THEN** 编译通过，`Item` 在该实现中绑定为 `int`

#### Scenario: 未绑定关联类型报错
- **WHEN** `class Bad : IIter { }` 未声明 `type Item = ...;`
- **THEN** 报诊断，指出缺少关联类型绑定

### Requirement: where 约束可指定关联类型绑定

#### Scenario: 绑定满足
- **WHEN** 写 `int Sum<C>(C c) where C : IIter<Item = int>`，用 `IntIter` 实例调用
- **THEN** 编译通过

#### Scenario: 绑定不满足
- **WHEN** 同上，但传入 `type Item = string;` 的实现
- **THEN** 报 `TypeMismatch`

#### Scenario: 跨包绑定往返
- **WHEN** 包 A 声明带关联类型的接口，包 B 实现并绑定，包 C 用 `where C : IIter<Item = int>` 约束
- **THEN** 校验在包 C 侧正确生效（绑定经 zbc TYPE 段持久化）

### Requirement: 泛型接口实例化（地基）

**Before:** `Z42InstantiatedType.Def` 是 `Z42ClassType` ⇒ 只能承载泛型 class/struct 实例化；
`Z42InterfaceType` 无型参名字段。`IEquatable<string>` 与 `IEquatable<int>` 在类型模型里无法区分。

**After:** 接口可被实例化并携带类型实参。

#### Scenario: 泛型接口保留类型实参
- **WHEN** 解析 `IEquatable<int>` 这个类型引用
- **THEN** 得到携带类型实参 `int` 的实例化接口类型，而非今天的裸 `Z42InterfaceType`

## IR Mapping（PR-3）

约束 bundle 新增 **bit7 = has_assoc_bindings**（bit0–bit6 已用满）：

```
cflags u8   ← bit7 新增
...（bit0-6 各块，同 PR-1）
[bit7] assoc_count u8
       (name_idx u32, type_idx u32) × assoc_count
```

接口的关联类型**声明**名单进 zbc TYPE 段的接口块。

⇒ **改 wire layout ⇒ zbc minor + zpkg minor 双 bump**，三方 reader（Rust `type_reader.rs` /
`ZbcReader` / `ZpkgReader`）同步 + 10 个 committed fixture regen。

## Pipeline Steps（PR-3）

- [ ] Lexer —— 不涉及（`type` 为上下文关键字）
- [x] Parser / AST —— `MemberParser` 拦截 `type Item;`；`TypeParser` 支持 `Name = Type`；`TypeExpr` / `Decl` 加节点
- [x] TypeChecker —— 泛型接口实例化地基；关联类型槽；绑定解析与校验
- [x] IR Codegen —— `ClassDescBuilder` 绑定进 `IrConstraintDesc`；`ZbcWriter` bit7
- [x] 元数据往返 —— `ZbcReader` / `TsigReconcile` / `ExportedInterfaceZ`
- [x] VM interp —— 运行期约束校验识别绑定（`generics.rs` 为判定 SoT）

---

## 共同的非目标（两 PR 都不做）

- **不改写真实源码使用新语法**：`INumber` / `Dictionary` / `Protocols` 保持旧写法，等下一个
  nightly 发布后另开 change 做 use 改写（bootstrap-seed 轴① 铁律）。测试用例不受此限。
- **不做类实现接口的成员签名齐备性校验**（今天不存在，见 design D5；独立 Deferred）。
