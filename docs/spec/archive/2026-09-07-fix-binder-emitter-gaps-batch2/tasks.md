# Tasks: 五条「binder 不认 / emitter 照发」缺口 —— 泛型基类链 / 元组可见性 / delegate 注册 / ns 限定静态调用

> 状态：🟢 已完成 | 创建：2026-09-07 | 完成：2026-09-07

**变更说明：** `restore-emit-zbc-diagnostics` 程序阶段 ⑥。清掉欠债表里 **B / B2 / B5 / B1 / B7**
五条真编译器 bug。它们同属 #507（阶段 ④）那一族：**binder 不认或元数据被抹掉，emitter 却照常发码**
——binder 报的错没人看见（`--emit-zbc` 吞诊断），emitter 那半边碰巧能跑，测试就绿。

**原因：** 三处「拿名字当键，却存了带修饰的名字」+ 两处「守卫 / 分支整条缺席」。

| 码 | 症状 | 根因 |
|---|---|---|
| **B** | `Bag<int> b = new SubBag<int>();` → E0402；继承自泛型基类的成员**运行期** `VCall: function Ns.Sub.Tag not found`（binder 零诊断） | `Z42ClassType.BaseName` 存**带泛型实参的基类源文本**（`"Bag<T>"`），而所有消费方拿它当 `Classes` 的键 ⇒ base 链在泛型基类处静默截断。发射端 `ClassDescBuilder` **同一行同一错法** ⇒ VM 建 vtable 找不到基类。另外 `Conversion` 的 G/H/I 三条独缺 **inst→inst** |
| **B2** | **任何** z42.core 之外的包写 `(1, 2)` → `E0404 cannot access internal class ValueTuple2` | `ValueTuple2..8` 无访问修饰符 → 默认 internal。编译器脱糖引用的其余类型均已 public，只漏这一组 |
| **B5** | 类型位 `Mapper<int,string>` → `E0443: undefined type: Mapper` | `StubCollector._passDelegates` 的 `TypeParams.Count == 0` 守卫把泛型 delegate 整条跳过；`IrGenAuxEmitter.EmitDelegates` 却照发 TYPE + `Invoke(...) -> R` |
| **B1** | 嵌套 `delegate` 声明**整体消失**（E0202 三连 + `undefined type: delegate`） | `MemberParser._parseMemberBody` 无 `delegate` 分支 ⇒ 落到 `_parseType()` 被当成一个名叫 `delegate` 的类型 |
| **B7** | `Std.IO.Console.WriteLine("hi")` → `E0401: undefined: Std`（裸名正常） | `_bindMemberCall` 三条静态分支都要求 target 是**裸** IdentExpr，限定形式的 target 是嵌套 MemberExpr ⇒ 全落空、直冲实例路径 |

**文档影响：** `docs/book/src/compiler/source-compile.md`（新增「名字与『拿名字当键』的三条纪律」——
B/B5/B1/B7 的机制与遗留限制 + 门的清单）、`docs/book/src/compiler/type-conversion.md`（分支表补
6a–6d，H2 = inst→inst 的判定与未覆盖面）、`docs/book/src/language/tuples.md`（`public` 是必需的
+ 假绿史）、`docs/design/language/delegates-events.md` §3.5（**过期校正指针**——那节写的是 C# 时代
的 `MemberType` 实现，且「类内部 simple-name 引用」在自举编译器里从未成立）。

- [x] 1.1 `z42.core/src/ValueTuple.z42`：`ValueTuple2..8` 补 `public`（B2）
- [x] 1.2 `StubCollector._passClassStubs`：`baseName` 改用已算好的裸名（B）
- [x] 1.3 `ClassDescBuilder._classDesc`：发射的 CLASS base 改用已算好的裸名（B，发射端半边）
- [x] 1.4 `Conversion._classifyBuiltin`：补 H2 分支 inst→inst（实参逐位规范同名 + `Def` 子类关系）+ `_sameTypeArgs`（B）
- [x] 1.5 `StubCollector._passDelegates`：去掉 `TypeParams.Count == 0` 守卫，签名走 `ResolveTypeP` 带 delegate 自身型参（B5）
- [x] 1.6 `MemberParser._parseMemberBody`：补 `TokenKind.Delegate` 分支（B1）
- [x] 1.7 `NestedFlatten._walkClass`：`DelegateDecl` 与嵌套 enum 同款提升为 `Outer+Inner`（B1）
- [x] 1.8 `SymbolTable.ResolveTypeP`：点串 `plusKey` 兜底补一条查 `Delegates`（B1）
- [x] 1.9 `TypeChecker`：新增 `_currentUsings` / `_collectUsings` / `_isVisibleNs`（逐 CU 采集 using ns 集）（B7）
- [x] 1.10 `MemberResolver._bindMemberCall`：ns 限定静态调用分支 + `_dottedPath` / `_rootIdent`（B7）
- [x] 2.1 `z42c.semantics/tests/typecheck/binder_emitter_gaps/`：11 条编译期门，**含 E0402 / E0401 / E0443 各一条负控**（钉住「断言 0 条」不是空门）
- [x] 2.2 `src/tests/generics/generic_base_inheritance.z42`：运行期门（泛型基类继承回归即 VCall 崩）
- [x] 2.3 `src/tests/delegates/generic_delegate.z42`：泛型 delegate e2e（含 void 返回 / 形参位 / lambda 赋值）
- [x] 2.4 `src/tests/classes/ns_qualified_static_call.z42`：ns 限定静态调用 e2e（含「真实例链不被劫持」）
- [x] 2.5 `src/tests/cross-zpkg/tuple_cross_pkg/`：B2 的**真门**（cross-zpkg runner 走 `z42c build`，诊断可见）
- [x] 3.1 文档同步（book 三页 + design 校正指针）
- [x] 3.2 GREEN（`xtask test` 全绿 + `test stdlib --mode jit` + 自举不动点 + `test bootstrap`）

## 爆炸半径：零字节影响（已核实）

全仓（`src/libraries` + `src/compiler`）**没有任何类继承自泛型 class 基类**——所有
`: IBasicCollection<T>` / `: ISubscription<TD>` 之类都是泛型**接口**，走接口分支、不碰 `baseName`。
也没有任何在用的嵌套 delegate、泛型 delegate、ns 限定静态调用（后三者今天写了就编不过）。元组在
z42.core 之外本来就编不过。⇒ 五条修复对现有产物字节**零影响**，无 golden / 格式 fixture churn，
无 format bump。

## 顺带接活的三条「假绿」测试

修前它们带着编译错误却"通过"——`--emit-zbc` exit 0 照写产物，而 emitter 那半边碰巧能跑：

| 测试 | 修前实况 |
|---|---|
| `src/tests/delegates/nested_delegate_dotted.z42` | **12+ 条**编译错误（`undefined type: delegate` ×2 + `undefined type: Btn.OnClick` ×4 + …）。2026-05-04 的 D-6 golden，假绿约四个月 |
| `src/tests/tuples/tuple_basic.z42` | E0404 ×N（每一种元组写法都撞） |
| `docs/design/language/delegates-events.md` §3.5 | 不是测试，但同一现象：整节「实现细节」描述的是已被迁移丢弃的 C# 实现 |

## 🔧 三处实施期校正（记录在案，勿重踩）

1. **欠债表的 bug B8 是误记**（「泛型类的字段初始化器不执行」）。造了 4 组合成用例——泛型 + 显式
   `new List<T>()`、泛型 + target-typed `new()`、泛型 + `where T : IShape` + `new()`（与
   `examples/oop.z42:87` **逐字同形**）、非泛型对照，再加裸名 / `this.` / 表达式体三种访问形式
   ——**全部正常，一个都没崩**。`examples/oop.z42` 确实崩（`ShapeCollection.Add` 行 89，`_shapes`
   是 Null），但包成最小工程走 `z42c build` 一看：**该示例根本不是合法 z42**，25+ 条错误
   ——表达式体属性（`public string Name => "Circle";`，z42 只支持 `{ get { return …; } }`）、
   `public T? Largest()` + `return default;`、以及 `using System;` / `using System.Collections.Generic;`
   （**z42 没有这两个命名空间**）。`_shapes` 恒 Null 的直接原因是 87 行的 `= new()` 报 E0437，
   而 E0437 的成因是 `List<T>` 本身未解析。**这是一个用 C# 语法写的过期示例，不是编译器 bug。**
   → 它为什么能烂着：`scripts/test/xtask_test_changed.z42:284` 明确 **`examples/` 一律 skip**，
   而 `xtask test examples` 管的是各 project 自己的 `examples/` 目录约定 ⇒ **仓库顶层 `examples/`
   没有任何门在编**。已按 User 裁决**本轮不处理**，见下「后续」。

2. **bug B 的记录（「`Conversion` 缺 Instantiated→Instantiated 分支」）只是表症。** 真根因是
   `BaseName` 存了带泛型实参的文本（任务 1.2/1.3）；补 H2 分支（1.4）只是其中一半。只补 H2
   的话继承成员照样找不到、运行期照样 VCall 崩。

3. **bug B1 的记录（「须补 `MemberType` AST 节点」）已不适用。** `add-nested-types` 后来建好的
   `Outer+Inner` 展平机制可以直接复用 ⇒ 实际修法是 3 行分支 + 1 行 plusKey 兜底，不需要新 AST 节点。

## 后续（不在本次 Scope）

- **B3-产出端**（欠债表最后一条）：让 SIGS 写真实限定名而非字面量 `"unknown"`。**不是遗漏，是刻意的
  C# 时代镜像**（`FunctionEmitter._sigTypeName` 注释：「限定名 → SIGS 输出 `unknown`（镜像 C#
  MemberType → `_` → "unknown"）」）。改它会动**几乎所有 stdlib 包的 SIGS 字节**（源码里签名位限定名
  粗估 ~107 处）+ 改反射行为（`FieldType.Name` 从 `"unknown"` 变真名）⇒ 字节基线门 / 格式 fixture
  需重刷。**User 裁决：拆独立 PR**，让字节 diff 可归因。
- 🆕 **限定名引用 imported 类整条是坏的**（写测试时撞出来，不是本次改动引入）：
  `new Std.Text.StringBuilder()` → 运行期 `VCall: function <ns>.<unknown>.Append not found`。
  根因：`SymbolTable.ResolveTypeP` 的限定名分支查 `ClassesByFqn`，而该视图**只由
  `StubCollector._putClassStub` 填（= 只有本地类）**，`ImportedSymbolLoader` 从不登记 ⇒ 任何
  `Ns.ImportedClass` 类型引用都落到 Unknown。`docs/design/language/attributes.md:189` 早就记过
  同一现象（C# 时代的 `ResolveMemberType` 版本）。与下一条同根，一并处理。
- **imported 类的 `Namespace` 保真度**：`Z42Type.z42` 的 `Namespace` 字段注释声称「导入由
  `ImportedSymbolLoader` 从 `em.Namespace` 设置」，**实际全文没有这条赋值**。补上会改 `Fqn()` 对所有
  imported 类的返回值 → 全仓发射面变动。这是 B7 从「窄修」升级到「能靠限定名消歧」的前提，也与
  memory 记的 R1/R3/R5/R7「`ImportedSymbolLoader` 类型保真度」同族。独立 change + DRAFT。
- **基类**实参**无处可存**：`class Sub<T> : Bag<string>` 与 `GBase<int> b = new CSub();`（非泛型派生
  → 泛型基类实例化）仍报 E0402。修得正确需在类符号上存「已解析的基类型」而非基类名字，属类型模型
  改动（且在 stub 收集期解析基类型有顺序依赖风险，见 common-pitfalls §1）。独立 change + DRAFT。
- **顶层 `examples/` 无人编译**：把 `examples/oop.z42` 改成合法 z42 + 加一道「编译顶层 `examples/`
  全部、有诊断就红」的 gate stage。同时会体检其余示例（大概率不止 oop.z42 一个烂了）。
- **同短名多 arity 的泛型基类**（`Classes` 键带 `$N`）：`BaseName` 存裸名仍对不上——与接口侧
  （同样存裸名）一致的既有限制，未处理。
- **多段 ns 限定的自由函数调用**（`Std.IO.SomeFreeFunc()`）：既有
  `fix-namespace-qualified-free-call` 只认单段（`mem.Target is IdentExpr`），本次未扩。
