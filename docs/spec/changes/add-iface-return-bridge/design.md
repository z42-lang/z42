# Design: 接口返回位的 struct 协变

> 状态：🔵 DRAFT 待审批 ｜ 前置：[proposal.md](proposal.md)

## 约束（先摆事实，三种形状都绕不过）

| # | 事实 | 证据 |
|---|---|---|
| C1 | sret 由**调用点的静态返回类型**决定 | `CallEmitter.z42:172` `_isBlobStruct(c.Type())` |
| C2 | **VCall 只按方法名索引 vtable 槽**，arity 不入解析键 | `CallEmitter.z42:168-171` 注释 |
| C3 | sret 是**每方法**的 `method_flags bit3` | zbc 1.40；`IrModule.z42:325` |
| C4 | VM 已按目标 flags 算 `expected = param_count + sret`，失配即抛 | `symres.rs:195-196` |
| C5 | 失配**必须**是错误，不能按 arity 自适应 | `symres_tests.rs:65` 专门断言「漏传 sret 槽不接受」——`fix-call-arity-skew` 的门 |
| C6 | struct→引用 的装箱机制**已有** | `__box_struct` builtin（`CallEmitter.z42:378` 已用它装箱 receiver）|
| C7 | z42 无显式接口实现语法 | 全仓零命中 |

C2 是关键：**同名桥接进不了同一个 vtable 槽**。C5 是另一个关键：**不能让 VM 凭 arity 猜**。

## D1（待 User 裁决）：第 2 件事的落法

### 形状 A —— 编译期合成桥接 + 接口派生派发键

具体方法 `GetEnumerator`（sret、裸名）保留给直接调用；另合成一个**接口派生键**上的桥接
（如 `IEnumerator$GetEnumerator`），内部调具体方法再 `__box_struct` 装箱成接口。
接口调用点按**接口声明**推导键 ⇒ 落在桥接上。

- ✅ 语义与 C# 显式接口实现等价；装箱只发生在接口边界，无装箱快路径不受影响。
- ✅ 不动 VM、不动 wire 格式（桥接就是一个普通方法）。
- 🔴 **接口调用点一律改按接口派生键派发** ⇒ ABI 本来就一致的绝大多数接口方法**也**要在该键上
  可达 ⇒ 要么每个实现方法都额外登记一个接口派生键（TSIG / 方法表膨胀），要么派发键推导分叉
  （「这个接口方法要不要走派生键」需要一个双方都算得出的判据）。**这是形状 A 的主要成本与风险**。
- 🔴 跨包：导入侧必须看到 zpkg 里已合成的桥接（不能在导入侧重新合成），否则跨包不一致。

### 形状 B —— 调用点标记 + VM 按目标 flags 适配（会动 wire 格式）

接口调用点在 VCall 上带一个「**允许 sret 适配**」标记（编译期知道：本调用点静态返回是引用型，
而目标可能是 struct 返回）。VM 在派发时若发现目标 `bit3=sret` 而调用未给槽，则**自己分配返回
blob 并装箱**交回调用点；无标记时维持 C5 的门（真 skew 照旧抛）。

- ✅ 不动派发键、不合成成员、方法表零膨胀 —— 对所有形状一次性正确（含未来泛型/跨包）。
- ✅ 门的判别力保住了：只在编译器明确说「这里合法」时适配。
- 🔴 **要动 wire 格式**（VCall 多一个标记位）⇒ zbc/zpkg minor bump ⇒ 两代自举 + 格式 bump 全流程。
- 🔴 语义下沉到 VM：「接口边界装箱」这件事从编译期可见变成运行期行为，调试面变差。

### 形状 C —— 收窄：这种协变直接报错（只做止血）

接口声明返回引用型、实现返回 blob struct ⇒ **编译错误**，要求实现把返回类型写成接口
（放弃该位点的无装箱）。

- ✅ 最小、最安全，一条 fix PR，立刻消灭静默崩溃。
- 🔴 **`List<T> : IEnumerable<T>` 仍然做不成**（除非让 `GetEnumerator` 返回接口 ⇒ 推翻
  `add-foreach-ienumerable` 的 Decision 8 无装箱前提）。User 已表示不想只做止血。

### 形状 A′ —— A 的便宜变体：**桥接占裸名槽，具体实现挪到合成名**

A 的成本被我第一版高估了。不必改派发键推导，可以反过来分配名字：

- **桥接占裸名** `GetEnumerator`（返回接口、无 sret）⇒ 正是接口派发按名字索引要的形状，
  **vtable 槽与派发键一个字都不用改**；
- **返回 struct 的具体实现放到合成名**（如 `GetEnumerator$struct`，沿用既有 `$` mangle 惯例），
  直接调用点由**编译期静态解析**绑到它 ⇒ 无装箱快路径保持零开销。

改动面从「派发键推导」降到「名字解析 + 一个合成成员」；要跟的是按名字判断的那几处
（`ChainHasMethod` / devirt / `ReceiverMethodIsVirtual`）。**这是形状 A 的推荐实现方式。**

## D1 结论（2026-09-25，User 追问「哪个更本质 / 更符合设计 / 性能更好 / 更利于扩展」后修正）

> **推荐形状 A（按 A′ 实现）。我撤回第一版对 B 的推荐** —— 那一版把权重放在「改动面小」，
> 而这恰是本仓明确不优先的维度（优先「改在对的层」）。四条轴里三条指向 A：
>
> | 轴 | 判断 |
> |---|---|
> | **更本质** | 「接口槽放引用 ⇒ struct 实现必须装箱」是**编译期可知**的事实，检查满足性时两侧都在手上。VM 在 VCall 只知道「传了 N、要 N+1」，不知道为什么、也不知道装箱是对的适配 —— B 得把这个事实经 wire 运过去让 VM 重新推导。**更关键：失配是「实现」的属性而非「调用点」的属性**（一个接口调用点会派发到多个实现），所以 B 的标记只能是「如果需要就适配」= **把每方法固定的 ABI（`bit3`）变成派发时协商的 ABI**，与架构审查主线「关键不变量靠约定 → 改构造式不变式」反向。|
> | **更符合设计** | 近乎决定性。本仓把「零新 IR / 零新发射 / 零格式 bump」当设计美德写在文档里（`tuples.md`「为什么零格式 bump」；`add-foreach-ienumerable` Decision 7 选「binder 脱糖成既有 AST」而非加节点；`syntax-customization.md`「后端对语法扩展一无所知」）。**前端合成桥接就是房子风格。** |
> | **性能** | A 更好，关键不在装箱（两者都装一次，语义必需）而在 **JIT**：A 保持 ABI 静态 ⇒ JIT 发固定调用序列，无装箱直接路径字节不变；B 让 sret 派发时协商 ⇒ JIT 要发条件化调用序列，否则落 `unsupported.rs` bail 名单 ⇒ **接口调用整体降级回解释执行**。 |
> | **后续扩展** | A 造出可复用原语「接口槽 → 一个不叫裸名的成员」，之后的显式接口实现 / 默认接口方法 / `ref` 返回 / 可空标记方向（`_checkNullableDirection` 已在处理同族的「MangleKey 命中 ≠ 契约兑现」）全要它。B 是点修，还要付一次格式 bump。 |
>
> 形状 C 仅作为「不想动机制」时的退路。**不建议形状 B。**

## 实现杠杆：三条在仓先例（2026-09-25 摸清，A′ 因此比第一版估计便宜得多）

| # | 先例 | 给 A′ 的用法 |
|---|---|---|
| L1 | **`emitKey ≠ methKey`**（静态 ctor，`IrGenMemberEmitter.z42:40-41`）：`emitKey = IrStaticCtor.MethodKey` **只改发射名**，体查找键仍用 `methKey` | 桥接就是同一形状：`emitKey = methKey + "$struct"`。⚠️ 那里白纸黑字警告「两者一起改会让 `model.HasBody` 落空 ⇒ 函数根本不发射（实测报 `static ctor C.$cctor not found`）」—— **体查找键绝不能跟着改** |
| L2 | **`SynthCctorClasses`**（`ClassDescBuilder.z42:604`）：下游判据取自 **IrGen 的实际发射记录**，而不是在下游重算一遍 | 桥接同样要让 IrGen 记一张「哪些方法被桥接了」的表，`ClassDescBuilder` 读它。该处注释的教训原话：「**不另拼一份**——手抄第二份键推导迟早漂移」 |
| L3 | **`RecordSynthEmitter` + `_out.AddFunc`**（`IrGenTypeEmitter.z42:112-130`）：合成 `Equals`/`GetHashCode`/`ToString` 为 IR 函数，**不进符号表** | 桥接函数照此合成；`__box_struct` builtin 已在用（`CallEmitter.z42:378` 装箱 receiver）|

⭐ **关键发现：`RegKey` 就是「注册键 = Methods 映射键 = IR 名 = BoundCall 目标名」的单一真相**
（`Symbol.z42:17-19`；`ClassDescBuilder.z42:584` 复述）。**直接调用在类收者上也发 VCall 裸名**
（`CallEmitter.z42:187` 的 `owns` ⇒「receiver 类链自有方法 → VCall，vtable 赢」）。
⇒ A′ 的「改名」是纯名字层面的事：**不动 VM、不动 wire 格式、不动派发键推导**，
直接调用点一行不改（它们本来就按 RegKey/发射名走）。

## D2：止血诊断的触发条件

无论取哪个形状，都保留一条「ABI 不自洽且**无法**桥接/适配」的错误出口。候选触发面：

- 实现返回 blob struct、接口声明返回**另一个 blob struct**（两者布局不同 ⇒ 装箱救不了）；
- 实现返回 blob struct、接口声明返回**基元**；
- 形状 C 下：实现返回 blob struct、接口声明返回引用型（即本 change 的主形状）。

新诊断码按纪律分配（扫 main + **每个在飞 PR 分支**的 `DiagnosticCodes.z42`），发射点先用字面量、
进 `diag-literal-emitters.txt` 挂账。

## D3：`ListEnumerator<T>` 侧要补的

声明 `: IEnumerator<T>` 后需对齐接口成员形态：

- `MoveNext()` ✅ 已有；`Dispose()` ✅ 已有；
- `Current` 是**属性** ⇒ 接口声明 `T Current { get; }`，prelude 侧登记的是访问器名
  `get_Current`（`BuiltinTypeDefs.z42:61`）—— 属性→`get_X` 的约定转换要确认两侧一致；
- ⚠️ **prelude 的 `IEnumerable<T>.GetEnumerator` 返回裸 `IEnumerator`**（丢了 `<T>`，
  `BuiltinTypeDefs.z42:57`，源声明是 `IEnumerator<T>`）。协变判定走
  `table.Implements(implName, wantRet.Name())` 用的是**裸名**，所以裸 `IEnumerator` 可能
  **恰好让判定通过** ⇒ **动它之前先确认方向，别把能用的弄坏**（独立立项，不在本 change）。

## 验收（判别力要求）

- **阴性**：本 proposal 的 10 行最小复现 —— 修前编译 EXIT=0 零诊断 + 运行期崩；修后要么正常跑
  （形状 A/B），要么给出明确诊断（形状 C）。
- **阳性对照**：**一字段** struct 返回那条（修前就通过）必须仍通过 —— 否则说明拦的是「struct
  返回」而不是「ABI 不自洽」。
- **端到端**：`List<T> : IEnumerable<T>` + `ListEnumerator<T> : IEnumerator<T>` 落地后，
  经 `IEnumerable<int>` 静态类型 `foreach`／传参／赋值三条都要真跑（形状 A/B）。
- **不回归**：`List<T>` 直接 `foreach` 仍走索引快路径（无装箱前提不变）。
