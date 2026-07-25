# DRAFT 提案：嵌套类型（nested types）作为语言特性

> 状态：✅ 已裁决（User 2026-07-25：D1–D5 照推荐、**不延后合理需求**、关闭 split-irgen-class 取 compiler 锁）
> 创建：2026-07-25 | 类型：lang（需规范先行）| 占用：`compiler`+`runtime`（已在 ACTIVE.md 登记）
> **scope 调整（User「全部都做，不要延后」）**：v1 纳入嵌套 class/struct/**interface/enum**，任意深度；
> 泛型外层 `Outer<T>.Inner` **试做**——若确需 0.5.x generic-instantiation 机制则如实标出（不静默延后）。

---

## 0. 为什么写这个提案

「嵌套类型反射」经实测证伪不是反射 polish：**嵌套类型在 z42 今天根本不能用**——
`class Outer { class Inner {} }` 里的 `Inner` 只被 parser 接受、被 `_checkNestedPartial`
扫一眼，**从不注册为可用类型、`Outer.Inner` 解析为 `<unknown>`、`new Outer.Inner()` 报错、
不发射 TYPE 条目**。故本提案的真实内容 = **把嵌套类型做成可用语言特性**，反射是收尾一小步。

---

## 1. 现状（实测）

| 维度 | 现状 |
|------|------|
| 语法 | ✅ parser 接受 `ClassDecl` 作为另一个 `ClassDecl` 的 member（AST 里 `outer.Members[i] is ClassDecl`） |
| 符号注册 | ❌ `SymbolCollector` 只迭代顶层 `cu.Decls` 的 `ClassDecl`；类体内只收 field/method/prop，**不递归注册嵌套 ClassDecl** |
| 名解析 | ❌ `Outer.Inner` → `<unknown>`（无此类型） |
| 实例化 | ❌ `new Outer.Inner()` → `new <unknown>`，报错 |
| IR/TYPE 发射 | ❌ `IrGen` 只迭代 `cu.DeclCount` 顶层 `ClassDecl`，嵌套的不发 TYPE record |
| 反射 | ❌ 无 `GetNestedTypes` / `GetDeclaringType` / `IsNested`；`GetMembers` 不含嵌套类型 |
| 唯一现有处理 | `_checkNestedPartial`：嵌套类**自身**标 `partial` → E0435（design D9，v1 不支持嵌套 partial） |

---

## 2. 关键设计决策（请 User 裁决 D1–D5）

### D1 — FQ 元数据名分隔符：`+`（C# 约定），避免 format bump ⭐

- **源码**用 `.`：`Outer.Inner ni = new Outer.Inner();`（自然、与 C# 源码一致）
- **元数据 FQ 名**用 `+`：`T.Outer+Inner`（`Type.FullName`），`Type.Name` = 简单名 `Inner`。
- **收益**：`GetNestedTypes` / `GetDeclaringType` / `IsNested` **纯运行期字符串派生**（找 `+`），
  **不加 TYPE section 字段 → 无 zbc/zpkg format bump**。与既有反射「无格式 bump」惯例一脉相承
  （数组 `[]` 后缀、嵌套泛型 `<>` 括号串——都靠名字约定而非新 wire 字段）。
- **消歧**：`.` 是 namespace 分隔、`+` 是嵌套分隔，二者不混 → `GetDeclaringType` 对 `T.Outer+Inner`
  切最后一个 `+` 得 `T.Outer`（真类型名，可解析回真句柄）；namespace 段永不含 `+`。
- **备选（不推荐）**：全 `.`（`T.Outer.Inner`）+ 新增 TYPE section「declaring-type-name」字段 →
  **触发 format bump + 自举 support/use 分阶段跨两个 nightly**（重）。选 `+` 规避整条重路径。

> **推荐：D1 = `+` 分隔（C# 约定，无格式 bump）。**

### D2 — 自举影响：零 ⭐（D1 的连带收益）

- 无 format bump（D1）→ 不踩 zbc/zpkg strict-pin 的格式轴。
- z42c / stdlib 源码**自身不使用**嵌套类型 → 不踩「新语法 use 晚一 nightly」轴（support 加进
  z42c、但 z42c 源不 use，上一 nightly 照编当前源）。
- 故**无自举分阶段负担**，单变更干净落地。（这也是选 D1 `+` 的关键动机之一。）

### D3 — v1 scope（哪些纳入、哪些延后）

**v1 纳入**：
- 嵌套 **class / struct**，**任意深度**（`A.B.C`，递归自然）。
- 名解析：`Outer.Inner` 限定名（任何位置）；`Inner` 非限定名（在 `Outer` 成员体内可见）。
- 实例化 / 字段·方法访问 / `typeof(Outer.Inner)`。
- 访问修饰：honor `public`/private——**跨包仅 public 嵌套可见**（对齐 C# 默认 nested private）。
- 反射：`GetNestedTypes()` / `GetDeclaringType()` / `IsNested`（+ `IsNestedPublic`/`IsNestedPrivate`
  可选）、嵌套类型纳入 `GetMembers()`（C# `GetMembers` 含嵌套类型，`MemberTypes.NestedType`）。

**v1 延后**（记 Deferred，不做）：
- **泛型外层**：`Outer<T>.Inner`（T 在内层可见性、构造型交互复杂）→ 延后。
- **嵌套 interface / enum** → 延后（先 class/struct 打通机制）。
- 嵌套 partial（已明确 v1 不支持，E0435 保留）。
- 同名遮蔽/`new`-式嵌套遮蔽基类嵌套类型 → 延后（v1 不继承嵌套类型枚举，`GetNestedTypes`
  只返**本类声明**的，对齐 C# `GetNestedTypes()` 默认只返当前类声明的，不含继承）。

> **推荐：D3 = 上述 v1 scope。** 若你想更小（例如先只 class、不含 struct），说一声。

### D4 — 名解析可见性规则（内层看外层）

C# 语义：嵌套类型体内可**非限定**引用外层类型名及外层的其他嵌套类型；外部须 `Outer.Inner`。
v1 采同规则：解析 `Inner` 时，作用域链 = 当前类的嵌套类型 → 外层类的嵌套类型（逐层上溯）→
本 namespace 顶层类型 → imported。**推荐 D4 = C# 同款词法作用域上溯。**

### D5 — `Type.Name` vs `FullName` 语义

- `Name` = `Inner`（简单名，与 C# 一致）。
- `FullName` = `T.Outer+Inner`。
- `GetDeclaringType().Name` = `Outer`，`.FullName` = `T.Outer`。
- **推荐 D5 = 上述**（镜像 C#）。

---

## 3. 实现草图（确认 DRAFT 后细化为 tasks.md）

### 3.1 编译器（`compiler` 锁）
1. **SymbolCollector**：递归注册嵌套 `ClassDecl`——遍历类 member 中的 `ClassDecl`，以 FQ 名
   `Ns.Outer+Inner` 注册进 class table；记录 declaring 关系（内层 `Z42ClassType` 挂 `DeclaringName`
   或直接靠 `+` 名派生）。递归到任意深度。
2. **名解析**：`Outer.Inner`（`MemberAccess`/`QualifiedName` 作类型位置）解析为嵌套类型的 FQ 句柄；
   词法作用域上溯（D4）解析非限定 `Inner`。`ResolveTypeName` / `TypeExpr` 解析路径扩展。
3. **IrGen / ClassDescBuilder**：把嵌套类型当作独立类**发 TYPE record**（FQ 名带 `+`）；其字段/方法
   /接口块与顶层类同构（复用现有 `ClassDescBuilder`）。`new Outer.Inner()`（`ObjNew`）用 FQ 名。
4. **codegen 名产出**：`Z42TypeName` 对嵌套类型产带 `+` 的 FQ 名（新增分支，类似接口的
   `QualifyClassName`）。

### 3.2 运行期（`runtime` 锁，小）
5. **reflection builtins**（`src/runtime/src/corelib/reflection.rs`）：
   - `__type_nested_types`（`GetNestedTypes`）：扫 type registry，返 FullName 形如 `<thisFQ>+<simple>`
     且 `<simple>` 不再含 `+`（即直接子嵌套）的类型 → name-only 或真句柄 `Std.Type[]`。
   - `__type_declaring_type`（`GetDeclaringType`）：本 FullName 切最后一个 `+` → 前缀解析回 Type；
     无 `+` → null。
   - `__type_is_nested`（`IsNested`）：FullName 含 `+`。
   - `GetMembers` 追加嵌套类型（与现有 methods/fields/props 切片并列）。
6. **Std.Type / Std.Reflection**（stdlib，`runtime` 邻接——实际改 `src/libraries/z42.core`）：
   加 `GetNestedTypes()` / `GetDeclaringType()` / `IsNested` 到 `Std.Type` 的 z42 门面。
   > ⚠️ 若改 z42.core 的**公开 API 面**，触发自举 axis ③（xtask/z42c 源新用该 API 要晚一 nightly）——
   > 但**这里只增不删、且 z42c 源不调用这些新反射 API** → 无 axis ③ 问题（加 API 随时可做）。

### 3.3 测试
- `src/tests/types/nested_types.z42`（e2e，interp+jit）：注册/实例化/字段方法/typeof/嵌套多层/
  跨包 public 嵌套/private 嵌套不可见/`GetNestedTypes`/`GetDeclaringType`/`IsNested`/`GetMembers` 含之。
- z42.core 反射单测补 `GetNestedTypes` 等。
- `xtask test compiler` 自举不动点 gen1==gen2 byte-identical（z42c 源不含嵌套类型 → 应零扰动）。

---

## 4. 文档落地（归档前）
- `docs/design/language/reflection.md`：把 `reflection-future-nested-types` 从 Deferred 移到已落地，
  记 `+` 名约定 + GetNestedTypes/GetDeclaringType/IsNested + GetMembers 纳入。
- `docs/design/language/`（类型系统页）：新增「嵌套类型」小节——语法、作用域、`.` vs `+`、v1 scope
  与 Deferred（泛型外层/嵌套 interface·enum/嵌套 partial）。
- 若涉复杂实现流程（名解析上溯 + FQ `+` 派生）→ book 对应机制页补实现原理（按 doc-system §5.1 判据）。

---

## 5. 风险与代价（认清）
- **compiler 锁争用**：`split-irgen-class` 持 compiler 锁；本 change 需 User 授权隔离并行分支
  （如既往 `add-z42-repl` / `infer-var-field-types` 先例）或排队等其归档。
- **名解析改动面**：类型位置的 `Outer.Inner`（`MemberAccess` 作类型）解析是 z42c 前端较敏感的一环，
  需仔细避免与 namespace-qualified 名（`Std.Console`）解析冲突——靠「先试类型链、`+` 内部约定」隔离。
- **不动点风险低**：z42c 源不使用嵌套类型 → 自举字节应零漂移；GREEN 以 CI 为权威。

---

## 6. 请 User 裁决
1. **D1–D5 是否照推荐**（尤其 D1 `+` 分隔 / D3 v1 scope）？
2. **compiler 锁**：授权隔离并行分支推进，还是排队等 `split-irgen-class` 归档？
3. 确认后我把本 DRAFT 收敛为 `tasks.md` 开始 IMPL。
