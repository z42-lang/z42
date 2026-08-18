# Design: add-property-getter

## Architecture

计算属性 getter 复用 **indexer** 已跑通的三段式 lowering，只是没有索引参数：

```
  源码   T Name { get { <body> } }
           │
  Parser   _parseProperty: get 后遇 '{' → _parseBlock() 捕获 GetBody      (Decl: PropertyDecl.HasGetBody/GetBody)
           │
  Symbols  get_Name MethodSymbol（已存在）+ 【不】合成 __prop_Name backing
           │
  Binder   DeclBinder: HasGetBody → 建 env(this + 全字段) → bindStmt(GetBody) → model.AddBody("<Class>.get_Name")
           │
  IrGen    HasGetBody → FunctionEmitter.EmitFunction 编译真实 get_Name 函数（0 参数、实例）
           │
  emit     x.Name → ExprEmitter 见 get_Name 方法存在 → VCall get_Name（既有派发，无需改）
```

对比 auto-property：`{ get; }` 走 `_emitAutoPropGetter` 合成 `field.get @__prop_Name` 空桩 + 合成 backing
field。计算属性两者都不要——getter 是真实体，member-access 已能派发到 `get_Name`。

## Decisions

### Decision 1: 复用 indexer 流水线，而非新建属性 getter 机制

**问题**：如何编译计算属性 getter 的 body？
**选项**：A — 新建一套属性 getter 编译路径；B — 复用 indexer 的 body → get_X 函数编译路径。
**决定**：B。indexer 已有完整的「GetBody 绑定（DeclBinder）→ 合成 MethodDecl → FunctionEmitter.EmitFunction」
链路，属性 getter 只是「0 参数的 indexer get」。复用 = 5 处镜像式局部改动，无新机制、无新 IR op、无 VM 改。

### Decision 2: 计算属性不合成 backing field

**问题**：auto-property 合成 `__prop_X` backing field（两处：SymbolCollector TSIG own-field +
ClassDescBuilder 运行时 layout）。计算属性要不要？
**决定**：不要。计算属性的值来自 getter 函数体，没有存储。两处都加 `!HasGetBody` 守卫。**漏一处**会残留
空字段 → TSIG/layout 与语义不符，且 member-access 若误绑 backing field 会读到 null（正是本特性要消除的 bug）。

### Decision 3: 保留源名 FieldSymbol（type-checking 视属性为字段）

SymbolCollector 仍注册源名 `Name` 的 FieldSymbol（`ct.Fields`，语义表，**不进 TSIG**），使 type-checking 把
`x.Name` 当字段访问看待。emit 端（ExprEmitter）见 `get_Name` 方法存在即派发 VCall，故读取正确走 getter。
不改 member-access 绑定/emit。

### Decision 4: get-only

计算 setter（`set { ... }`）不在本变更——反射用例只读。`set` 仍走 auto（`set;`）。写 `set { }` 会在
`_expectSemi` 报错（可接受，out of scope）。

## Implementation Notes

- **body 键一致性**：DeclBinder 用 `ctKey + ".get_" + Name` 存 body，IrGen 用 `c.Name + ".get_" + Name` 取
  （非泛型类 ctKey==c.Name；泛型 arity-mangle 时两侧对称，镜像 indexer get_Item 键）。
- **getter env**：`this` + `ct.Fields` 全部（含其它属性的源名 FieldSymbol，harmless），无索引参数、无 `value`。
- **synthPg MethodDecl**：`new Param[0], 0`（0 参数）、hasBody=true、body=GetBody、retType=pd.Type；
  `EmitFunction(..., isInstance=!static, isStatic=static, owner, ...)`。
- **bootstrap 边界（关键）**：计算属性是**新语法**。z42c 自身源码 + stdlib 本 PR **不使用**它（`Type.z42` 的
  use 是后续 change），故种子 z42c 编 z42c-源/stdlib-源时不会遇到新语法。若后续 use 与本 support 并成一个 PR，
  需实测冷启动种子 mis-compile 是否影响 bootstrap（见 proposal Open Question）。

## Testing Strategy

- **parser/AST golden**（`z42c.syntax/tests/property_getter/`）：`{ get { body } }` 解析成带 GetBody 的
  PropertyDecl（`HasGetBody=true`），auto `{ get; }` 不变。
- **e2e golden**（`src/tests/types/computed_property.z42`）：计算属性读取返回 getter 计算值（含引用其它字段 /
  其它属性 / 表达式），且**无** backing field 残留（对比 auto-property 的存储语义）。
- **回归**：既有 auto-property + indexer 用例保持绿（`xtask test` 全 stage）。
- **自举**：z42c 5/5 gen1==gen2（本 PR 不在 z42c 源用新语法 → 种子可编 → 不动点保持）。
