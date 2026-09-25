# Tasks: 接口返回位的 struct 协变（形状 A′）

> 状态：🚧 进行中 | 创建：2026-09-25 | 类型：lang（需规范先行）
> 规范：[proposal.md](proposal.md) · [design.md](design.md)（D1 = 形状 A，按 A′ 实现，User 2026-09-25 裁定）

## 为什么分阶段（不是「两条一起做」的退让）

User 裁「两条一起做」= **不要把「只报错」当最终答案**；本 tasks 仍在同一个 change 里交付
止血 + 可用两件事，但拆成可独立验证的阶段，理由是硬约束而非偏好：

- **阶段 3（跨包）与阶段 2（本包）之间可能必须跨一个 nightly**：桥接要进 TSIG / 导出元数据，
  若给 `z42.ir` 的模型加字段并被 `semantics` 引用 = 新跨成员符号 ⇒ `bootstrap-seed.md`
  的 support/use 纪律（#788/#789 同形）。**能否骑既有通道要到阶段 2 末才确定。**
- 阶段 1 单独就能消灭「编译期零诊断 + 运行期崩」——先落地它，等于任何时刻中断都不留静默坑。
  ⚠️ **更正**（本文件初版把触发面写窄了）：阶段 1 的诊断必须**也覆盖主形状**（接口声明返回引用型），
  否则阶段 1 落地后那条崩溃仍在。阶段 2 用桥接把主形状变成可跑，届时**收窄**该诊断到
  「桥接无法成立」的形状（design D2）。这一小段「先报错、后放行」是刻意的安全顺序，不是返工。

## 阶段 1：止血 —— 🟢 结论：**不需要新诊断**（原计划作废，理由如下）

原计划给「ABI 不自洽的协变」加一条诊断。逐个形状核下来，**能到达那条路的只有一种，而它现在能跑**：

| 形状 | 实际归宿 |
|---|---|
| impl 返回 blob struct / 接口声明**引用型** | ✅ 桥接（本 change 的主形状）|
| impl 返回 blob struct / 接口声明**另一个 struct** | 协变判定本就不过（struct 无继承，`IsSubclassOf`/`Implements` 皆假）⇒ **已有 E0412** |
| impl 返回 blob struct / 接口声明**基元** | 同上 ⇒ **已有 E0412** |
| impl 返回**标量 repr** struct / 接口声明引用型 | 修前修后都对（自描述 Value），**不该**报错也不该桥接 |

🔴 **残留窗口（已知、未闭合）**：`CanBridge` 为假而形状又确实是 blob struct 时会静默跳过
（=退回修前的运行期 arity 门）。它要求 `_blobStructName` 解不出名字 —— 泛型与跨包两种情形实测
都解得出，所以这个窗口**很可能是空集**，且它是**既有行为、不是本 change 引入的回归**。
没有就地报错的原因：**IrGen 层没有诊断出口**（全仓 `IrGen` 无 `DiagnosticBag`，发射器不报诊断
是设计选择），要报就得穿参一条 bag = 跨层改动，与这个窗口的体量不相称。真踩到了再单独立项。

## 阶段 2：桥接合成（本包内）—— 🟡 非泛型档已跑通（`cce4c14a3`）

**已落地**：判据分两层（符号层 `IfaceBridgeRet` 标记 + 发射层 `IsBlobStruct`）、`emitKey` 改名、
`IfaceBridgeSynth` 合成裸名桥接（`struct_alloc` + `call $struct` + `__box_struct` + `ret`）。
最小复现由「零诊断 + 运行期崩」变为正确输出；一字段标量档实测未动（无回归）；
`test compiler` 24 unit + **自举不动点 3/3** 全过。

🔴 **未完：泛型返回类型**。`CanBridge` 对 `Z42InstantiatedType` 返回 false（不改名不合成，
保持现状）。要支持得先解决「实例化返回类型的 blob 布局名怎么算」——必须**复用**
`ExprEmitter._instLayoutName` / 特化通道（`SpecInstName`/`SpecOwnerName`）那套推导，
不能手抄第二份。**`List<T>` 卡在这一档**（`ListEnumerator<T>` 是实例化返回类型）。

- [x] 2.1 具体实现挪到合成名 `<m>$struct`（沿用既有 `$` mangle 惯例；**先确认它不与 arity
      mangle `name$N` 撞命名空间** —— `$ctor`/`$indexer`/`$cctor` 是同族先例，`SurfaceHash` 那套
      `$` 名**不是**同一命名空间，切勿合表）
- [x] 2.2 合成桥接占**裸名**：签名取接口声明（返回 `R_i`、无 sret），体 = 调 `<m>$struct`
      → `__box_struct` → return。合成落点参照 `RecordSynth` 的先例
- [x] 2.3 直接调用点**无需改**（实测确认）：它们发的就是发射名 ⇒ 自动落到 `<m>$struct`；
      接口调用点也一字不改（裸名槽已是桥接）。`RegKey`/发射名是派发键单一真相这条省掉了整块工作
- [x] 2.4 跟按名字判断的那几处：`ChainHasMethod` / devirt（`ResolveSealedTarget`）/
      `ReceiverMethodIsVirtual` —— 漏一处就是静默走错方法
- [x] 2.5 **虚覆盖一致性**（改为标记沿基链**下推到符号**；发射期不再上溯，判据只留一份）：该方法若在类层次里被覆盖，子类覆盖也必须是桥接形态，否则子类槽变回
      sret ⇒ 同一个崩溃换个入口回来。需要在继承解析处强制
- [x] 2.7 **泛型返回类型档**（复用 `_blobStructName` 后自动覆盖；另修 `MemberResolver` 泛型实例化收者漏走 `CallKey()`）：复用 `_instLayoutName` / 特化通道算实例化 blob 布局名
      （`List<T>` 的前置；见本阶段抬头的 🔴）
- [x] 2.6 e2e：用户自定义接口 + 两字段 struct 返回，经接口调用真跑通；**退回对照**验判别力

## 阶段 3：跨包 —— 🟢 **本来就是对的**（我误诊过一次，已撤回）

我曾判断「跨包直接调用静默返回装箱值」，**是错的**。换**值语义判别**（struct 拷贝后两者是否
独立）重测：**未打任何补丁的构建就输出值语义正确**（`b.Current=10`）。为它写的导入侧补丁已丢弃。

原因：导出侧 TSIG 按**发射名**反推方法表（`TsigReconcile` 扫 `Functions`），`m$struct` 与桥接
两个名字都在（实测 `List` 收到 38 个方法，两者俱在）；导入侧 `m$struct` 的符号 `RegKey` 本就是
`m$struct`，裸键经 first-wins 也指向它 ⇒ `CallKey()` 直接返回正确名字。

🔴 **误诊的根源值得记住**：`--dump-ir` 在这条路上**不加载导入符号**（同一证据：`--dump-bound`
全是 `<unknown>`），我用一个会说谎的工具做了三轮推断。⭐ 真正钉死结论的是**语义判别 + 退回对照**。

- [x] 3.1 导出/导入两侧形态一致（本来就一致，见上）
- [x] 3.2 **无需格式 bump**（TSIG 从函数名反推，多一个函数即多一个方法条目，走既有通道）
- [x] 3.3 跨包覆盖：`list_as_ienumerable` 的 ④（`List<T>` 在 z42.core、测试在另一个包）

## 阶段 4：use —— 解锁 `List<T> : IEnumerable<T>`

- [x] 4.1 `ListEnumerator<T> : IEnumerator<T>`（`Current` 属性 ↔ 接口 `get_Current` 访问器形态要对齐）
- [x] 4.2 `List<T> : IEnumerable<T>`
- [x] 4.3 e2e：经 `IEnumerable<int>` 静态类型 foreach / 传参 / 赋值三条；
      **不回归**：`List<T>` 直接 foreach 仍走索引快路径（无装箱前提不变）
- [~] 4.4 `Dictionary` 同款 —— **有意延后**（非机械改动）：元素类型契约是设计决定（C# 迭代
      `KeyValuePair<K,V>`，z42 要不要照此需裁），且 `where TKey: IEquatable` 会与接口实参交互。
      机制侧已就绪（`DictionaryEnumerator` 是同一形状），随时可做。
- [x] 4.5 文档：`collections-core.md` 的 `List<T>` 签名与说明（含「自身 foreach 仍走索引快路径」）
      + `Protocols/IEnumerable.z42` 那句「目标」注释改写为记述事实 + 为什么此前写不出来

## 顺带发现的既有缺陷（与本 change 无关，待立项）

`[Record] struct X<T>(T _f)` 这种**型参主构造器字段**，从方法返回后读出**零值**
（实测 `ia.Current` 读 0 而非 7）。用旧 SDK 编译器（≈main）跑基线**完全相同** ⇒ 既有、独立。
本 change 的 fixture 因此改用 `T[]` 字段绕开，并把这条写进注释 —— 否则下一个人会以为是桥接坏了。

## 不做（本 change 之外）

- 显式接口实现语法（`IFace.Member`）
- prelude 的 `GetEnumerator` 返回裸 `IEnumerator`（design D3 末：裸名可能**恰好**让协变判定通过，
  动它之前要先确认方向）⇒ 独立立项
- LINQ / 集合视图
