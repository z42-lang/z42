# Design: 强制访问权限检查

## Architecture

```
成员访问绑定（MemberResolver / OverloadBinder / ExprTyper）
   │  解析出成员符号（FieldSymbol / MethodSymbol）+ 声明类（短名 / Z42ClassType）
   ▼
AccessChecker.CheckAccess(vis, declClassShortName, env, symbols, kind, name, span)
   │  vis=="public"    → OK
   │  vis=="private"   → env.CurrentClass()==declClass ? OK : E0404
   │  vis=="protected" → CurrentClass()==declClass 或 CurrentClass() 派生自 declClass ? OK : E0404
   │  vis=="internal"  → declClass.IsImported==false ? OK : E0404
   ▼
违规 → _diags.Error(E0404, msg, span)（不阻断绑定，继续产 Bound 保后续诊断）
```

## Decisions

### Decision 1: 名字形式 —— 全短名，直接可比

**问题：** `env.CurrentClass()` 与成员的声明类名是否同形，能否直接 `==`？
**事实（探索确认）：** `env.CurrentClass()` 返回 `ct.Name()`（短名，DeclBinder 构造 TypeEnv 时传入）；
`FieldSymbol.ContainingTypeName` / `MethodSymbol.ContainingTypeName` 均由 SymbolCollector 从 `c.Name`
（短名）设置。类在 SymbolTable 以裸名注册。
**决定：** 直接用短名比较，无需 FQN 归一。同短名跨 ns 碰撞是既有 first-wins 现象（sealed-devirt 已记录），
本变更沿用现有 keying，不额外处理（属既有语义层约定）。

### Decision 2: local/imported 判定 —— 给 Z42ClassType 加 IsImported（根因修复）

**问题：** `internal` 需判「声明类是否本包」，但 TypeChecker/MemberResolver 无 imported/local 信息，
`SymbolTable.Classes` 把两者混存，无 origin 标志。
**选项：**
- A（消费端启发式）：在检查点查 `ImportedSymbols.Classes.ContainsKey` —— 但 MemberResolver 拿不到
  ImportedSymbols，需层层穿线；且散落多处。
- B（产出端根因）：`Z42ClassType` 增 `bool IsImported`，import 加载（ImportedSymbolLoader 构造 imported
  类型）时置 true，本地类型默认 false。声明类自己携带来源。
**决定：** 选 **B**。符合「改产出端让数据从源头正确」（philosophy 根因修复）；origin 是类型的固有属性，
放类型对象上最自然；检查点一行 `declClass.IsImported` 即可，零穿线。

### Decision 3: 不 bump zbc/zpkg 格式（实证确认——含跨包 internal）

**问题：** 加 IsImported + 真正 emit E0404 + **跨包 internal** 是否触发格式变更？
**决定：** 否。三点：
1. `IsImported` 由 import 加载时按来源现算、**不序列化**（每次编译按本次依赖图重定）。
2. **跨包 internal 无需 bump**（实现期实证纠正初判）：成员可见性早已是格式里的 `u8`（0=public/1=private/
   2=protected）。给它加值 **3=internal** **不改字节布局**——Rust reader 只原样携带 `u8`（`loader.rs`
   无穷举 match 拒 3），反射 `IsPublic=(vis==0)`/`IsPrivate=(vis==1)` 比较对 3 仍正确。故只改两个编码函数
   （`IrGenFacts._visCode` 无修饰符→3、`TsigReconcile._visStr` 3→"internal"），**零格式常量改动**，跨包
   internal（字段+方法）即生效（实证：z42.uri 跨 zpkg 访问 z42.text internal 成员被 E0404）。
3. 强制层是纯诊断，合法程序 Bound/IR 字节不变。

> **反射语义副作用（正确化，非回归）**：无修饰符成员现 emit vis=3 → 反射 `IsPublic` 对其返回 false
> （此前 vis=0 误报 true）。C# 语义下 internal 本就非 public，故这是**修正**。若旧反射 golden 断言
> 无修饰符成员 IsPublic=true，按新语义更新。

### Decision 3b: override 继承基类可见性（消除无修饰符 override 地雷）

**问题：** 代码库 ~99 处无修饰符 `override`（ToString/Stream/Dump…）被判 internal → 跨包调用 E0404。
**事实：** C# 中 override 不能改基方法可见性；只能覆写 virtual/abstract 成员（基类契约，通常 public）。
**决定：** `_vis` / `_visCode` 对**无显式访问修饰符**的 `override` 返回 public（继承基类）。一条规则消除全部
~99 处，零源码 churn，是正确 C# 语义（非补丁）。widening 方向安全（永不误报 E0404）。

### Decision 3c: record 定位字段公有

**问题：** `record R(string A, ...)` 的定位字段经 `new FieldDecl("", ...)`（空 mods=internal）合成 →
跨包访问（CompileRequest/Target/Dirs 等 DTO）E0404。
**决定：** `DeclParser` 合成 record 定位字段用 `"public"` mods（镜像 C# record 定位参→public 属性）。

### Decision 3d: 编译器 split 辅助类互访 private → internal（45 处）

**问题：** z42c 的 split-parser（Parser/DeclParser/MemberParser/TypeParser）等辅助类互访 `private` 方法
——C# 非法（private=声明类文本）。强制后 self-build 断（无强制期遗留的欠标注）。
**决定：** 把这些**同包协作**辅助成员 `private→internal`（45 处，跨编译器全包）。internal 是同包互访的
正确 C# 修饰符；纯 widening（不改运行语义、不引入误报）。stdlib 私有零违规（本就规范）。

### Decision 4: private 语义 —— 类文本级，非实例级；不随继承开放

镜像 C#：`private` 可及域 = 声明类文本。判据 `CurrentClass()==ContainingTypeName`。
- 同类其它实例：`CurrentClass()` 仍等于声明类 → 通过。
- 派生类经基类实例访问基类 private：`CurrentClass()`=派生类 ≠ 基类 → E0404（`_findField` 沿基链**能找到**
  该 private 字段，但 CheckAccess 随后拒绝——名字查找与可及性分离，正是 C# 行为）。

### Decision 5: protected 语义 —— 沿派生方基链上溯

判据：`CurrentClass()==declClass` 或从 `CurrentClass()` 沿 `HasBase`/`BaseName` 用 `symbols.GetClass`
上溯能到 `declClass`。`GetClass` 对本地 / imported 基类均可解析（同一 Classes 图），故跨包派生天然支持。
`CurrentClass()==""`（自由函数上下文）→ 不派生 → protected 判 E0404（自由函数无「派生」身份）。

### Decision 6: internal 与 CurrentClass 无关，只看声明类来源

`internal` 可及性由「声明类是否本包」决定，与访问点在哪个类无关：
- 自由函数（`CurrentClass()==""`）访问**本包**类的 internal → `IsImported==false` → 通过（正确：同包）。
- 访问 **imported** 类的 internal → E0404（正确：跨包）。

### Decision 7: 违规不阻断绑定

emit E0404 后仍返回原本的 Bound 节点（沿用现有 `E0404` 之外诊断的「报错后继续」风格，如 UndefinedSymbol
路径），使单次编译能收集多条诊断、不因一处 access 违规掩盖后续问题。

## Implementation Notes

**强制点（每处：解析出成员 → 取 vis + 声明类 → CheckAccess）：**

| 位置 | 成员形态 | 声明类来源 |
|------|---------|-----------|
| `MemberResolver._bindClassMemberAccess` 字段命中 | 实例字段读 | `fs.ContainingTypeName` |
| 同上 getter 命中（`get_<Name>`） | 属性读 | getter `ms.ContainingTypeName` |
| 同上方法组命中 | 方法值 | `mg.ContainingTypeName` |
| `MemberResolver._bindInstanceMemberCall` Z42ClassType 分支 | 实例方法调用 | `ms.ContainingTypeName` |
| `MemberResolver._bindMember` 静态字段（BoundStaticGet） | 静态字段读 | 目标类短名 |
| 静态方法调用绑定处（OverloadBinder / _bindCall，实施期定位） | 静态方法调用 | `ms.ContainingTypeName` |
| 属性 setter `obj.P = v`（ExprTyper/StmtBinder 赋值路径，实施期定位） | 属性写 | setter `ms.ContainingTypeName` |

- **声明类 Z42ClassType 获取**：`symbols.GetClass(declShortName)`；`internal` 检查读其 `IsImported`。
  null（理论不应发生）→ 保守当本地（不误报）。
- **protected 上溯**：复用 `_findField` 同款 `HasBase`/`BaseName`/`GetClass` 遍历，含隐式 Object 回落。
- **消息措辞**：`cannot access {vis} {kind} '{name}' of '{class}'`；internal 加 `from another package`。
- **不碰 wrapper/prim/接口/泛型形参路径**：prim 包装类、接口成员均 public API，无 private；泛型形参
  成员松绑 Unknown，无符号可查 → 不接入（避免误报）。

**AccessChecker 归属**：新文件 `AccessChecker.z42`（< 200 行），持 TypeChecker backref 取 `_diags`
（与 MemberResolver 同款 mediator）。避免把 MemberResolver 撑过行数限。

## Testing Strategy

- **单元 / golden（z42c.semantics/tests/access-control/）**：private 类外读/写、同类其它实例、派生访基类
  private、protected 派生 OK、protected 无关类 E0404、internal 同包 OK；每条断言 E0404 有/无 + 消息。
  注意 `SemanticDump.ErrorCount` 只计 parse+TypeChecker diags——CheckAccess 经 `_diags`（TypeChecker
  bag）emit，故走常规 ErrorCount 断言即可（区别于 SymbolCollector.Diags，见 memory
  semanticdump-errorcount-skips-collector-diags）。
- **跨包 e2e（src/tests/cross-zpkg/access-internal/）**：A 包 internal 成员，B 包访问 → 编译期 E0404。
- **自举字节不动点**：`xtask test compiler` gen1==gen2（纯诊断层不改产物的硬校验）。
- **完整 GREEN**：`xtask test`（cargo build + e2e + cross-zpkg + stdlib + compiler + vscode-syntax）。
  —— **stdlib / 自举是 internal 强制的真实破坏面探针**：若出现跨包 internal 漏网访问，此处红，按
  workflow 中断条件停下汇报。
- **REPL 复验**：编译器落地后，用新 z42c 重建 REPL 依赖，手验 `class A{private int a;} A a=new(); a.a` 报 E0404
  （REPL 走 scripting 独立绑定路径，不在门禁，需单独验——见 memory add-target-typed-new）。
