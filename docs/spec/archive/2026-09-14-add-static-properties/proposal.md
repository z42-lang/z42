# Proposal: 静态属性 + 类内裸名静态成员 + 属性初始化器

> 类型：lang（编译器语义层；无新语法、无 IR 指令、无 zbc/zpkg 格式改动）
> 来源：迭代清单 ①b（User 2026-09-13 裁决：紧接表达式体成员之后另开 change）

## Why

`static` 属性在 z42 里**任何写法都静默错编**（2026-09-14 用 nightly `2843abebe` 编译器实测，interp/JIT 一致）：

| 写法 | 今天的结果 |
|---|---|
| `St.K`（`static int K => 5;` / `{ get {..} }` / `{ get; set; }`） | 发 `static_get Demo.St.K` ⇒ 运行期 `MissingSymbolException: static field Demo.St.K is not declared`，**编译零诊断** |
| 静态 auto 属性的 `get_A` / `set_A` 桩 | 0 参静态函数里发 `field_get %0.__prop_A`（把不存在的 `this` 当接收者） |
| `St.A = 7` | 发 `static_set Demo.St.A`（不存在的字段） |
| 类内裸名 `K` / `A` | `E0401 undefined` |
| 计算静态 getter 体里引用 `this`/实例字段 | 编译期被当实例上下文绑定（体绑定时无条件 `Define("this")`） |

全仓（`src/` `examples/` `scripts/`）**零处**声明静态属性 ⇒ 从没人走过这条路。

实测时同一探针又挖出三个**同根**缺陷——它们都是「类内/限定名访问静态成员」与「属性存储」这两条路径上的洞，
静态属性要正确工作必须经过它们，因此一并纳入：

1. **类内裸名引用静态字段**：实例方法里 `cnt`（`static int cnt = 40;`）编译通过、**运行读回 Null**
   （体绑定把 `ct.Fields` 全部——含 static——定义成变量，发射端按实例字段发 `field_get reg0.cnt`）；
   静态方法里同样的 `cnt` 报 `E0401`。**`const` 字段同样中招**（`const` 隐式 static）：实例方法里裸名 `M` 读回 Null。现有用例全部写成 `Counter.count`，所以没被发现。
2. **限定静态成员复合赋值** `St.cnt += 2` 报 `E0401 undefined: St`：`AssignTyper` 的 event `+=`/`-=` 拦截分支
   先把接收者当表达式绑定；`=` 分支早有 `staticRecv` 跳过，`+=` 分支漏了。`St.cnt++` 是好的。
3. **属性初始化器从来没生效**：`public int P { get; set; } = 12;` 读回 `0`，`{ get; } = "nm"` 读回 `null`
   ——实例属性也一样，有无显式 ctor 都一样。parser 解析了 `HasInit/Init`，**语义层零读取点**（字段初始化器
   `int x = 11;` 是好的）。文档 `member-accessors.md` 却写着「初始化器：给后备字段一个初值」。全仓只有
   parser 单测和 `examples/oop.z42`（本就在编不过名单里）用到。

不做会怎样：静态属性是 C# 常用写法（单例 `Instance`、配置 `Default`），一旦有人写就是运行期才炸；
裸名静态字段在实例方法里**静默读 Null** 是比 E0401 更坏的失败形态；属性初始化器更是文档承诺的功能静默失效。

## What Changes

- **静态属性全形态可用**：auto（`{ get; set; }` / `{ get; }`）、计算（`{ get {..} }` / `=> e` / `get => e`）、
  初始化器 `= e`；使用位 `C.P` 读、`C.P = v`、`C.P += v`、`C.P++`/`--`、类内裸名 `P`（读写）、跨包 `C.P`。
- **类内裸名静态字段**：实例 / 静态方法、ctor、属性 getter、索引器体内裸名 `f` ≡ `C.f`（C# 语义）；
  局部 / 形参遮蔽优先。
- **`C.f += v` 修复**（字段与属性同一路径）。
- **属性初始化器生效**（实例 + 静态）：实例 → 注入 ctor 体首（与字段初始化器同序、按声明序交错）；
  静态 → 进 `__static_init__` / 有静态 ctor 时注入其体首（与静态字段初始化器同一套机制）。
- **诊断**：静态 get-only auto 属性只能在**本类静态 ctor** 内赋值，否则 `E0452`；静态计算属性任何位置赋值 `E0452`
  （对称实例规则）。计算属性写初始化器 `int P => 1 = 2` 本就语法不可达；`int P { get { .. } } = 1` 报 `E0452`（无存储可初始化）。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|---|---|---|
| `src/compiler/z42c.semantics/src/BoundExpr.z42` | MODIFY | `BoundStaticGet` 加属性访问元数据 |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | `C.P` 绑定：本地属性 FieldSymbol / 跨包 static `get_P` 方法 |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | 裸名 → 本类静态字段/属性 ⇒ `BoundStaticGet` |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 体绑定不再把 static 成员定义成变量；静态 getter 不定义 `this`；属性初始化器注入（实例/静态） |
| `src/compiler/z42c.semantics/src/AssignTyper.z42` | MODIFY | `+=` 分支跳过静态接收者；静态属性 E0452 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` / `AccessEmitter.z42` / `OperatorEmitter.z42` | MODIFY | 属性型 `BoundStaticGet` → `call get_P` / `call set_P` |
| `src/compiler/z42c.semantics/src/StubEmitter.z42` | MODIFY | 静态 auto 桩改发 `static_get/static_set Q(C).__prop_P` |
| `src/compiler/z42c.semantics/src/MemberCollector.z42` | MODIFY | 静态 auto 属性登记后备名（`PropBackingName`） |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | `ctx.Fields` 不再收 static 成员（裸名走绑定期改写） |
| `src/compiler/z42c.semantics/src/ClassExtractor.z42` | MODIFY | TSIG static 方法段导出静态属性访问器（跨包） |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY（若需） | 跨包静态访问器导入（实施时以 cross-zpkg 用例为准） |
| `src/compiler/z42c.semantics/tests/typecheck/static_property_tests.z42` | NEW | 负例：E0452 / 裸名遮蔽 |
| `src/tests/classes/static_properties.z42` | NEW | 正例全形态（interp + JIT） |
| `src/tests/classes/static_fields_bare_name.z42` | NEW | 裸名静态字段（实例/静态方法/ctor/getter） |
| `src/tests/classes/property_initializers.z42` | NEW | 实例 + 静态属性初始化器 |
| `src/tests/cross-zpkg/<新 case>` | NEW | 跨包静态属性读/写 |
| `docs/book/src/language/member-accessors.md` | MODIFY | 静态属性节 + 初始化器语义 + 实现原理 |
| `docs/book/src/language/`（静态成员所在页） | MODIFY | 裸名静态成员解析规则 |
| `docs/spec/changes/add-static-properties/**` | NEW | 本变更规范 |

## Out of Scope（各自 loud 失败、非静默，登记为后续）

- 经派生类名访问基类静态成员 `Derived.baseStatic`、派生类内裸名引用基类静态成员（今天 `E0401`）
- 静态索引器、静态 event、计算 setter `set { .. }`（实例也未支持）
- 泛型类的静态属性 `G<T>.P`（先实测静态字段在泛型类上的现状，若同样可用则顺带覆盖，否则登记）
