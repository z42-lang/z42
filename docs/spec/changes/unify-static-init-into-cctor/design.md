# Design: 静态初始化统一到类型初始化器

> 配套 [proposal.md](proposal.md)。本页讲**怎么做**与**为什么这样做**。

## 1. 目标形态

全系统只有一个静态初始化概念：**类型初始化器（type initializer）**，在该类型首次使用前执行。

```
        改前                                    改后
┌─────────────────────────┐          ┌─────────────────────────┐
│ 无 cctor 的类           │          │                         │
│  → per-CU __static_init__│          │  一切静态初始化器       │
│  → 包加载后批量跑        │   ───►   │  → 宿主类型的类型初始化器│
├─────────────────────────┤          │  → 首次使用该类型前      │
│ 有 cctor 的类            │          │                         │
│  → 类型初始化器          │          └─────────────────────────┘
│  → 首次使用前            │
└─────────────────────────┘
```

## 2. 编译器侧

### 2.1 绑定期（DeclBinder）

现状二分支（[DeclBinder.z42:180-192](../../../../src/compiler/z42c.semantics/src/DeclBinder.z42) / `:232-236`）：

```
静态字段/静态 auto 属性有初始化器
  ├─ 宿主类有显式 static ctor → 注入 cctor 体首
  └─ 否则                     → 进 per-CU __static_init__
```

改为无分支：**一律注入宿主类型的类型初始化器体首**。宿主类没有显式 `static C()` 时，
合成一个（体为空，只有被注入的初始化器）。

`SemanticModel.AddStaticInit(cls, field, init)` 的签名不变——**每条记录本来就带宿主类名**，
这正是本变更成立的前提。消费端从"按 CU 线性展开"改为"按 `SiCls` 分组"。

### 2.2 发射期（IrGen / FunctionEmitter）

- 删 `IrGen.Generate` 的 "static_init 首位" 特判与 `SourceStem` 依赖。
- 删 `FunctionEmitter.EmitStaticInit`。它的两条特殊约定一并消失：
  - "不带 DBUG 行表/局部表"——理由是与已移除的 C# bootstrap 编译器字节对齐，已无存在意义。
  - `_boxIfStaticStruct` 的专用转发——统一走 `AccessEmitter._emitStaticStore`（已装箱）。
- 合成类型初始化器沿用现有 `$cctor` 发射名（`$` 非法于标识符 → 与无参实例 ctor 的 `C$0` 零撞名）。

> ⚠️ 发射名与 `RegKey` 的关系不得动。`methKey` 兼作 SemanticModel 的体查找键，
> 两者一起改会让函数根本不发射（add-static-constructors 踩过）。

### 2.3 哨兵（ClassDescBuilder）

`$Cctor` 哨兵的挂载条件从「有显式 static ctor」扩为「**有显式 static ctor 或有静态初始化器**」。
载荷不变（cctor 的发射函数名 FQ）。**零格式 bump**——类级 attr-ref 哨兵机制已在（先例 `$Deprecated`）。

### 2.4 编译期屏障消除（新增 pass）

这是本变更的性能关键，形状直接对标 `CtorKnownFixup`（zbc 1.39）。

**位的语义：`owner_init_free` —— 编译期证明该站点的 owner 类型没有类型初始化器。**

| 位 | 运行期行为 |
|---|---|
| `true` | **不发屏障**，直接访问 |
| `false`（含缺席） | 走完整运行期屏障 = 今天的行为 |

**方向是刻意的：缺席即保守态。** 反过来编码（位表示"owner 有初始化器"）会让"证不出来"
落到**免检**一侧——一旦判错就是**静默跳过类型初始化**，比它要省的开销坏得多。这条纪律
与 `CtorKnownFixup` 的「为什么是正向位」同源，见
[CtorKnownFixup.z42:29-35](../../../../src/compiler/z42c.pipeline/src/CtorKnownFixup.z42)。

三条从 `CtorKnownFixup` 直接移植的实施约束：

1. **必须等到整包装配之后**。发射那一刻答案不存在：本 CU 的 `IrModule` 看不到同包其它文件，
   而 `Deps` 按设计不含本包。
2. **跨包站点一律不置位**。编译时依赖 v1 的 `C` 无初始化器 → 置免检 → 运行时装到 v2 的 `C`
   有初始化器 ⇒ 静默跳过。这是 `dep-version-skew` 那条线的老对手，不给它新入口。
3. **每次装配重算全部站点**（不是只置位、不是 OR）。否则增量缓存里的 `IrModule` 会留下过期结论
   ——同包某个类刚加了静态字段，缓存里的站点还标着免检。

覆盖面：绝大多数类根本没有静态字段 ⇒ 绝大多数站点可证 init-free ⇒ 屏障代码根本不发射。

## 3. 运行期侧

### 3.1 屏障：全局门 → per-TypeDesc 原子字节

现状（[cctor.rs:79-92](../../../../src/runtime/src/vm_context/cctor.rs)）：

```
CctorRegistry { map: Mutex<FxHashMap<String, CctorEntry>>, pending: AtomicUsize }
热路径：if pending != 0 { 切 owner 名字 → type_registry 哈希查表 → map 上锁再查表 }
```

`pending` 恒为 0 才让它"几乎免费"，而**恒为 0 的前提是全仓没人用 cctor**（实测：stdlib +
编译器显式静态构造器 **0 个**）。本变更把 519 条静态初始化器的宿主类全部纳入，
**这个前提立即失效**，且短程序里很多类型一辈子不被触达 ⇒ 门整个进程关不上。

改为：**状态住在 `TypeDesc` 上的一个 `AtomicU8`**。

```
热路径：td.init_state.load(Relaxed) == Done ? 直接过 : 慢路
```

- `ObjNew` **早已是这个形状**（手上有 `TypeDesc`，`td.cctor_func()` 一次 `Option` 判断就结束）。
  本变更把另外三个触发点对齐过去，不是发明新机制。
- 静态字段读写与静态调用拿 owner `TypeDesc` 的代价已经付过了——字段名/函数名在**函数首次执行时
  预解析**，把 owner td 一并缓存进解析结果即可，热路径不再碰字符串。
- 全局 `pending` 计数、`owner_class_of_static_field` 的字符串切分、`map` 的 Mutex 全部退役。
- CLR / JVM 同样把初始化标志放在方法表 / 类元数据里，而非全局注册表。

状态机不变（`NotRun → Running(ThreadId) → Done | Failed`），同线程重入仍直接放行。
`Failed` 的异常文案是少见路径，继续放旁挂表（见 proposal Open Question 2）。

### 3.2 删除 `__static_init__` 平行管道

| 删除项 | 位置 |
|---|---|
| 加载期 `name.ends_with(".__static_init__")` 全函数表扫描 | `lazy_loader/registry.rs:72` |
| `pending_static_inits` 队列 | `lazy_loader.rs:85` |
| `static_init_state` / `InitState::Claimed` 认领窗口 | `lazy_loader.rs:86`、`lazy_loader/resolve.rs:183` |
| `running_static_inits` 计数 | `vm_context/types.rs:63` |
| `run_pending_static_inits` 的双队列循环（塌缩为单队列） | `vm_context/lookup.rs:265-325` |
| `collect_lazy_static_init_names` | `jit/mod.rs:253` |
| `METHOD_STATIC_INIT` 常量 | `metadata/well_known_names.rs:100` |
| `init_static_fields` 的 eager 枚举 + 排序 | `interp/entry.rs:95-107` |

登记点改为**统一漏斗** `build_type_registry`：读 `$Cctor` 哨兵即登记，急切合并与惰性加载
共用同一入口。这条现状已经成立（static-ctor-init.md「登记点」），本变更只是让它成为唯一路径。

### 3.3 初始化失败：从打死进程改为可捕获

现状 `__static_init__` 抛出 → `bail!("uncaught exception in static init ...")` **直接终止进程**
（[interp/entry.rs:103-106](../../../../src/runtime/src/interp/entry.rs)）。

统一后走类型初始化器的既有语义：类型标记 `Failed`，后续访问抛
`Std.TypeInitializationException`（可捕获，原始消息在 `Message` 里）。**纯改善**，
但属于可观察语义变更，需在 reference 记一笔。

## 3.4 「读到零值而非报错」的由来——以及为什么本变更才是它的修复

实测发现：`A.X = B.Y + 1` 在 B 尚未初始化时得到 `1` 而非报错，`static int Raw = B.Y` 得到 `0`。
根因**不是**回归，而是一个**本身正确**的既有修复：

[`symres.rs::verify_static_field`](../../../../src/runtime/src/vm_context/symres.rs)
（`fix-static-value-field-null-slot`）——值类型静态字段的槽位由
`resize_with(|| Value::Null)` 填 Null，没人按声明类型零初始化，于是 `static int N;`
一读就崩在 `__box_prim: expected integer value, got Null`。该修复按声明 `type_tag` 补零值。

这个补零**必须保留**：`static int N;`（无初始化器）读到 `0` 是正确语义。

但它有一个副作用——**把「初始化器还没跑」也一并粉饰成了合法的零值**。于是
`__static_init__` 的声明序缺陷从「抛 type mismatch」退化为「静默给 0」。
这正是 [[silent-feature-masks-other-bugs]] 的形状：一个静默兜底盖住了另一个真 bug。

**本变更就是它的修复**，而且是唯一正确的修复方向：统一之后读 `B.Y` 必先跑完 B 的类型
初始化器，「读到未初始化状态」这个状态本身不再存在，补零只会落在真正没有初始化器的字段上。

残留的两种「值类型静态读到零」在统一后仍然合法，且都已被 spec 覆盖：

| 情形 | 语义 | spec |
|---|---|---|
| 字段声明了但无初始化器 | 读到声明类型零值 —— 正确 | 既有行为，不变 |
| 初始化器抛异常 ⇒ 类型 `Failed` | 读到 `TypeInitializationException`，**不是**零值 | 场景 6 / 7 |
| 同线程重入（cctor 递归触发自身） | 可能看到部分初始化状态 —— C# 语义 | 既有行为，不变 |

> ⚠️ **不要试图"修复"补零逻辑本身**。把它改回抛错会让 `static int N;` 重新崩溃。
> 正确的边界是：消灭"读到未初始化状态"这个状态，而不是让读到它时更吵。

## 4. 顺序语义

**规范上不承诺跨类型的初始化顺序**（对齐 CLR / JVM）。

这不是让步，是修正。现状 per-CU 按**声明序**线性展开：`class A { static int X = B.Y + 1; }`
声明在 `class B` 之前时，`A.X` 读到未初始化的 `B.Y`。统一之后顺序由**真实依赖**决定——
A 的初始化器读 `B.Y`，那次读上的屏障先跑完 B 的初始化器，拿到的是拓扑序（同 Go 的 init）。

> 实施上仍按原声明序注册类型，使"无依赖关系的类型"的相对顺序与今天一致，减少行为漂移面；
> 但**规范不锁死**，留出将来优化空间。

**风险**：全仓 519 条静态初始化器中若有依赖"同文件声明序"的，行为会变（多数是变对）。
逐条扫描列为实施前置（tasks.md 阶段 7.0）。

## 5. 决策记录

| # | 决策 | 理由 |
|---|---|---|
| D1 | 不保留 `beforefieldinit` 式双策略 | User 裁决（2026-09-22）：统一到"首次使用调用"。双策略的性能理由被 D2 消解 |
| D2 | 屏障成本靠"编译期证明 + per-TypeDesc 原子字节"解决，不靠全局门 | 全局门的"免费"前提是全仓没人用 cctor，本变更必然打破它 |
| D3 | 屏障消除位方向 = "证明无初始化器 → 免检" | 缺席即保守态；反向编码会新引入"静默跳过初始化" |
| D4 | 跨包站点不置屏障消除位 | 依赖换版本后编译期结论过期 ⇒ 静默跳过 |
| D5 | 零格式 bump | `$Cctor` 是类级 attr-ref 哨兵，只是更多类挂上它 |
| D6 | 跨类型顺序规范上不承诺 | 对齐 CLR/JVM；惰性+屏障天然给出依赖拓扑序，强于声明序 |

## 6. 风险与缓解

| 风险 | 缓解 |
|---|---|
| 519 条初始化器里存在声明序依赖 | 实施前置逐条扫描 + 保留"按声明序注册"减少漂移面 |
| 函数条目数 per-file → per-class，zpkg 体积上涨 | 量测；#418 实测执行本身只占 78µs，成本在"找"不在"跑" |
| 屏障改在派发面 | GREEN 必含 `xtask test stdlib --mode jit`（[[local-green-misses-jit-and-lines]]） |
| bench 门误报 | 编译器类改动被判红按 [[investigate-micro-ab-false-regressions]] 做字节对账证伪 |
| 自举：新编译器产出的 zpkg 要能被上一版 VM 读 | 零格式 bump ⇒ 无 wire 变更；但 `$Cctor` 挂载面扩大属**语义**变更，需确认上一版 VM 遇到更多哨兵不炸 |
