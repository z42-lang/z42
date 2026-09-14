# Design: add-static-properties

## 现状诊断（为什么今天全错）

静态属性在 `MemberCollector` 里**已经**登记成 `FieldSymbol(IsStatic, IsProp)` + 静态 `get_P`/`set_P` 方法符号，
`ClassDescBuilder` 也**已经**给静态 auto 属性发了静态后备字段描述 `__prop_P`。断在三处：

1. **使用位**：`MemberResolver._bindMember` 的「类名.成员」分支只看 `sfs.IsStatic` ⇒ 属性被当字段 ⇒ `BoundStaticGet`
   ⇒ 发 `static_get C.P`。实例属性靠发射端 `_isPropMember` 改道到 `VCall get_P`，静态这边**从来没有对应的改道**。
2. **访问器桩**：`StubEmitter._emitAutoPropGetter/Setter(isStatic=true)` 只把形参数改成 0/1，指令仍是
   `FieldGet/FieldSet(reg0, "__prop_P")`。
3. **裸名**：体绑定（`DeclBinder` 5 处）把 `ct.Fields.Keys()` **全部**——含 static / const——`env.Define` 成变量，
   `FunctionEmitter` 同样把全部字段塞进 `ctx.Fields` ⇒ 实例方法里裸名静态字段 = `field_get reg0.x`（读 Null）；
   静态方法不定义任何字段 ⇒ `E0401`。

属性初始化器：`PropertyDecl.HasInit/Init` 在语义层零读取点；所有注入点（合成 ctor / `_injectFieldInits` /
per-CU `__static_init__` / `_injectStaticFieldInits`）只遍历 `FieldDecl`。

## 决策

### D1：属性访问仍用 `BoundStaticGet` 表示，节点上携带属性元数据（不新增 Bound 节点）

```
BoundStaticGet {
  ClassName, FieldName, Type
  + PropGetter : string   // "" = 普通静态字段；否则 "get_P"
  + PropSetter : string   // "" = 无 setter
  + PropBacking: string   // 静态 auto 属性后备名 "__prop_P"；计算属性 ""
}
```

**理由**：`BoundStaticGet` 已被读、赋值、`++/--`、复合赋值、const/readonly 检查、逃逸分析、各分析 walker 识别。
新增节点类型意味着每个 walker 都要补分支，**漏一个就是静默跳过**——正是本仓反复出现的失败形态
（见 memory `silent-feature-masks-other-bugs`）。扩字段只需改**发射**分叉点，walker 零改动。

**备选（否决）**：绑定期直接改写成 `BoundCall get_P`（实例跨包属性的做法）——读可以，但 `C.P++` / `C.P += v`
需要可写左值，`BoundCall` 不是左值，得在 `OperatorEmitter` / `AssignTyper` 各造一套读-改-写，反而分散。

元数据在**绑定期**填好（本地来自 `FieldSymbol`，跨包来自导入的静态访问器方法符号），发射端与
`AssignTyper` 只读节点、**不再回查符号表**——跨包时根本没有 `FieldSymbol` 可查。

### D2：发射——属性型 `BoundStaticGet` 委托给既有静态调用发射

| 位置 | 普通静态字段（不变） | 属性（新） |
|---|---|---|
| `ExprEmitter` 读 | `static_get` | `Emit(BoundCall("static", C, get_P, []))` |
| `AccessEmitter._emitAssign` | `static_set` | 有 setter → `Emit(BoundCall("static", C, set_P, [v]))`；无 setter 且有后备（仅静态 ctor 合法，E0452 已把关）→ `static_set C.__prop_P` |
| `OperatorEmitter` 前/后缀 `++/--` | `static_set` 写回 | 同上 setter 写回 |
| 复合赋值 | `AssignTyper` 脱糖 `x = x op v`，读写两侧各走上两行 | 同左 |

委托 `BoundCall` 而非手发 `CallInstr`：跨包静态调用的 FQ 限定（`CallEmitter` 里 `Deps` 解析）已经在那条路径上，不重写第二份。

### D3：访问器桩

`_emitAutoPropGetter/Setter` 的 `isStatic` 分支改发 `StaticGetInstr(dst, Q(C).__prop_P)` / `StaticSetInstr(Q(C).__prop_P, reg0)`；
调用方传入限定后备名。`MemberCollector` 给静态 auto 属性填 `PropBackingName`，但**不** `AddOwnField`
（那是实例布局，保持 `!static` 门）——今天两件事共用一个 `pfHasBacking`，拆开。

### D4：类内裸名静态成员——绑定期改写成 `C.x`

```
_bindIdent(id):
  t = env.LookupVar(id)                  // 局部 / 形参 / this / 实例字段
  if t != null → 原路径
  sfs = 本类(env.ClassName).Fields[id]   // 新增，位于 enum 类型名 / 自由函数判定之前
  if sfs != null && sfs.IsStatic
     → 与 MemberResolver「C.x」分支同一构造（访问/弃用检查 + 属性元数据）
  ...原 enum / HasFunc / E0401
```

配套：`DeclBinder` 各体绑定处 `env.Define` 只定义**非 static** 字段；`FunctionEmitter` 的 `ctx.Fields` 只收非 static。
这样实例方法里的裸名静态字段不再命中实例字段路径，局部/形参遮蔽仍由 `LookupVar` 先命中天然保证；
lambda 内裸名静态成员不是捕获（`LookupVar` 不命中），语义正确。

两条路径（`C.x` 与裸名）抽成 `MemberResolver` 上一个 `BindStaticMember(ct, name, span, env)`，**判据只有一份**。

静态计算 getter 的体绑定环境不再 `Define("this")` 与实例字段（今天无条件定义）。

### D5：`C.x += v`

`AssignTyper` event `+=`/`-=` 拦截分支补上与 `=` 分支相同的 `staticRecv` 判定（接收者是未被遮蔽的类名 → 跳过拦截），
落到通用复合赋值脱糖。静态 event 仍不支持（Out of Scope），行为与今天一致之外只少了一条错误的 E0401。

### D6：属性初始化器

注入点**不新增**，在既有 4 个字段初始化器循环里加 `PropertyDecl` 分支（同一循环 ⇒ 字段与属性**按声明序交错**，R3）：

| 注入点 | 字段（现状） | 属性（新） |
|---|---|---|
| 合成 ctor（无显式实例 ctor，root→self） | `this.f = init` | 写 `this.__prop_P` |
| `_injectFieldInits`（显式实例 ctor，非 `this(..)` 委托） | 同上 | 同上 |
| per-CU `__static_init__`（无静态 ctor） | `AddStaticInit(C, f, init)` | `AddStaticInit(C, "__prop_P", init)` |
| `_injectStaticFieldInits`（有静态 ctor） | `C.f = init` | 写 `C.__prop_P` |

**直接写后备存储，不经 setter**（对标 C#：初始化器是字段初始化，派生类 override 的 setter 不参与；get-only 属性没有 setter 也能初始化）。
实例写法构造 `BoundAssign(BoundMember(this, "__prop_P"))`——`AccessEmitter` 对不在 `Fields` 里的成员名按原名发 `field_set`，恰好落到后备字段；
右值绑定复用 `AssignTyper` 的尾段（target-typed / 隐式转换检查 / 装箱 / 表示转换），抽一个 `BindInitValue(init, targetType, env)` 供字段与属性共用。

四个循环的「成员 → (是否有初始化器, 目标名, 目标类型)」判定抽成一个 helper，避免四处各写一份属性分支。

计算 / extern 属性写了初始化器（若 parser 接受）→ `E0452`「no storage to initialize」。

### D7：跨包（实施校正）

DRAFT 原判断「TSIG static 方法段不导出访问器 ⇒ 导入侧看不到」**不成立**：跨包符号今天由
`TsigReconcile.Rebuild` 从依赖 zpkg 的 **IR 函数签名**重建（drop-tsig-expt P3 后 TSIG 段已删），
pass 2 把本类**全部**静态函数（含 `get_X` / `set_X`）列为静态方法 ⇒ 导入侧本来就有静态访问器符号。
退回对照实测：关掉 `ClassExtractor` 的导出，跨包正例照样通过。

真正缺的只有**导入侧的使用位**：`MemberResolver.BindStaticMember` 在无 `FieldSymbol` 时按
`get_X`（IsStatic）构造属性型 `BoundStaticGet`、按 `set_X` 定可写性；发射经 `_depStaticTarget`（DepIndex）限定。
导入类拿不到后备名 ⇒ 跨包给只读静态属性赋值的 E0452 统一是「it has no setter」措辞（本包 get-only 是
「outside its static constructor」）。

`ClassExtractor` 仍补导出静态访问器：让进程内导出模型（`SemanticDump` / 同包多 CU）与 `TsigReconcile`
的重建结果一致（实例属性 getter 早就导出），不是跨包可用的前提。

无 zbc / zpkg 格式改动。

## 字节影响与普查（实施必做）

- 静态属性：全仓零使用 ⇒ stdlib / z42c / xtask 产物**应零字节漂移**。
- 裸名静态/const 成员：**若**现有源码在实例方法里用了裸名静态字段 / const，今天它们是 Null 静默 bug，本改动会**改变其字节**（且修好它）。
  实施前做 census pass：把 D4 判据搬成只打印、挂在绑定点，跑 stdlib + z42c 全量构建，列出所有命中点逐个确认。
  命中为零 ⇒ 断言零字节漂移；非零 ⇒ 每处写进 tasks.md 并加回归用例。
- 自举纪律（bootstrap-seed）：本 change 只落「支持」。**z42c / stdlib / xtask 源码在下一个 nightly 发布前不得使用**
  静态属性、类内裸名静态成员、属性初始化器——上一版 nightly 会把它们静默错编。

## 文档

- `docs/book/src/language/member-accessors.md`：新增「静态属性」节；修正「初始化器」语义（直接写后备、按声明序、先于 ctor 体）；
  「实现原理」小节写 D1/D2/D6 的流程（mermaid：绑定期元数据 → 发射分叉）。
- 静态成员语义页（实施时定位：`static-constructors.md` 或 classes 页）：补「类内裸名解析顺序」。
- `examples/oop.z42` 仍卡别的 C#-ism，名单不动（双向棘轮会自己报）。
