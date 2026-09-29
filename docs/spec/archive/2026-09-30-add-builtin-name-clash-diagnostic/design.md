# Design: E0499 —— 类型名与内建基元拼写冲突

## 三种症状（实测，main `dddf8df76` 重建工具链）

同一个根因，三副面孔。**任何一种都没有诊断。**

| # | 形态 | 结果 |
|---|---|---|
| ① | `struct Single { public int X; public int Y; }` + 写字段 | 运行期 `FieldSet: expected object, got F64(0.0)` |
| ② | `struct Single<T> { … }`（泛型） | 运行期 `struct field write out of blob bounds` |
| ③ | `[Record] struct Single(int X, int Y)` | **编译器自己崩**：uncaught exception，零诊断 |

⭐⭐ ③ 崩在 **#938 的符号化守卫**上：
`struct field path: layout lookup failed (index -1) — the field is not in the layout table;
baking this would have produced a wrong offset`（`AccessEmitter.z42:557`）。
⇒ 那道守卫**已经知道这里不对**，只是它的位置只能抛，给不出用户级诊断。
E0499 要做的就是**在它之前、在声明位**把话说清楚。

## Decision 1：落点 = `DeclEnforcer`，新增 `_passBuiltinNameClash(cu)`

`DeclEnforcer` 的定位就是「声明良构约束簇 …… 纯语法/AST 级检查」，
已有的 `_passAttributeSuffixEnforce` / `_passAnalyzerSuffixEnforce`（E0444/E0445/E0447）
是**同一形状**：遍历 `cu.Decls` → 看 `ClassDecl.Name` → 发一条「改名」错误。

新 pass 与它们**并排挂在同三个调用点**（`SymbolCollector.z42:76 / :331 / :385`）。

❌ **不挂在 `StubCollector` 的 E0458 处**：那里在 stub 注册循环里，拿不到干净的
`CompilationUnit.Namespace`，而豁免判据正需要它。

## Decision 2：判据 = 问 `PrimModel`，**不复制那张 14 名表**

```
PrimModel.Code(PrimModel.Canon(name)) >= 0
```

- `Canon` 把 BCL 拼写折成关键字（`Single`→`float`、`String`→`string`、`Object`→`object`）。
- `Code(canon) >= 0` 覆盖**全部 14 个**（`IsScalarValue` 只到 11，**漏掉 `string` / `object`**
  —— 而实测那两个同样会崩，判据必须用 `Code` 而不是 `IsScalarValue`）。

⭐⭐⭐ **这条是本设计最重要的一点**：缺陷的根因就是 `Canon` 的折叠行为，
所以判据必须**问 `Canon` 本人**，而不是照着它当前的值域抄一张名单。
抄名单就是结构审计 **R2「判据复制」**再添一处 —— `Canon` 将来增减一个拼写，
诊断会静默地跟不上（多一个 ⇒ 漏报并回到今天的崩溃；少一个 ⇒ 误拒合法代码）。
规范里也按这个口径写：**表是 `Canon` 的当前值域，`Canon` 才是唯一真相源。**

## Decision 3：豁免 = `cu.HasNamespace && cu.Namespace == PreludeNs.Root()`

⚠️ **实施期更正（2026-09-30）**：原写「== `"Std"`」。并入 main 后发现 #949~#952 期间新增了
`z42c.semantics/src/PreludeNs.z42` —— **prelude 命名空间的唯一 SoT**，其头注写的正是
「同一个事实此前在三处各写一遍字面量 …… 三份拷贝各自漂移正是本仓反复失手的形状」。
⇒ 用 `PreludeNs.Root()`，**不再写第四份字面量**。这与 Decision 2（判据问 `Canon` 本人、
不抄名单）是同一条原则。

⚠️ 用 `Root()` 而**不是** `Contains()`：`Std.Runtime` 也是 prelude ns，但那 14 个包装类型
声明在 `Std`（根）。而 `Std.Runtime` 里若有个叫 `Single` 的类型，它的符号表键仍是裸名
`Single` ⇒ 照样被折 ⇒ **它应当被诊断**，不该豁免。

不用 `SelfPkg` / `DepScan.IsPrelude`：

| 方案 | 否决理由 |
|---|---|
| `SelfPkg == "z42.core"` | 单文件 `--emit-zbc` 没有工程 ⇒ `SelfPkg` 为空，**而那正是本缺陷最容易发生的场景**；靠它豁免等于把门关在最需要它的地方 |
| 「本次编译没链 stdlib 就不报」 | 仓里确有这种**宽松闸门**先例（`StmtBinder._chkCatchType`），但它会让门在**一整类编译路径**上失效 |
| **`Namespace == "Std"`** ✅ | `Canon` 自己就剥 `Std.` 前缀 ⇒ 语言早已认定 `Std.X` 是这些名字的家；判据是纯 AST 的、零依赖 |

**逃生口已经堵着**（实测）：

- 用户往 `namespace Std` 声明**新**类型（`Std.MyThing`）→ 合法，正常工作。`Std` 不是保留命名空间。
- 用户往 `namespace Std` 声明 `Single` → **现有 E0606**（本包遮蔽导入）当场报，
  因为那 14 个 FQN 在 `z42.core` 里都存在。**本规则不重复管辖。**

## Decision 4：范围 = `ClassDecl` 且 `Kind != "interface"`

`ClassDecl.Kind ∈ {"class","struct","interface"}`（`z42c.syntax/src/Decl.z42:254`），enum 是独立的 decl 类型。

实测：`enum Single` 与 `interface Single` 行为**正确**，不受折叠影响 ⇒ 不报。
为它们加约束是在凭空扩大语言限制。

⚠️ 泛型 `class Single<T>` 实测**正常**（class 不走 blob 布局），但泛型 `struct Single<T>` **崩**。
判据按 `Kind` 一刀切（class + struct 都报）而不是按「会不会崩」：

- 「会不会崩」依赖 blob 布局的实现细节，是个**会变的**判据；
- 而「这个名字归 `Std`」是条**可陈述的语言规则**；
- 且 `class Single` **非**泛型时实测也崩 ⇒ 按 kind 排除 class 会漏报。

## Decision 5：嵌套类型靠**名字里的 `+`** 天然豁免，但必须有测试钉住

`_passAttributeSuffixEnforce` 的注释写明它跑在 **NestedFlatten 之后**，
届时嵌套类型的 `Name` 是 `Outer+Single` ⇒ `Canon` 折不动 ⇒ 不报。实测也确认嵌套形态不崩。

🔴 **这是一条「按构造成立」的豁免，没有任何代码表达它** ⇒ **必须写测试钉住**。
否则哪天 flatten 的拼写一变，范围会**静默**扩大（误拒）或缩小（漏报），
而两种都不会有人发现。

## Decision 6：Error，且挂在**声明位**

- **Error 不是 warning**：今天这形态 100% 导致运行期崩、错值、或编译器崩，没有「有意为之」的合法写法。
- **挂声明位**：`struct Int32 { public int X; }` 只要**从不碰字段**今天恰好不崩 ——
  但把诊断挂在「碰了才报」上，规则就不可陈述、还依赖「有没有正好触发崩溃」。
  声明即报。

## 消息措辞

照 E0458 `DuplicateTypeName` 与 E0444 后缀族的口吻：陈述冲突 + 给出路。

```
type `Single` collides with the built-in type `float` — `Single` is its BCL-style
spelling, and outside namespace `Std` that name is reserved (a type declared with it
is silently treated as the primitive: its layout is never computed, so field access
crashes at run time). Rename it.
```

## 不做（与 proposal 一致）

- **不改 `PrimModel.Canon`**：让折叠变成命名空间感知的才是根治，但 `Canon` 的结果同时是
  **派发键 / 查找键**，在热路径上、全仓消费。收益是「让 `Demo.Single` 可用」这个几乎无人
  需要的能力，风险是全局的。
- **不碰 `ZbcWriter` 无条件写空 struct 块**：它现在是**信号**（#947 的字段表覆盖门正是靠
  `size = 0` 把本缺陷抓出来的）。E0499 落地后它不再可达，但删它是另一件事。
