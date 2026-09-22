# Design: 包级初始化回调 `[ModuleInit]`

> 本文件是本变更的 **SoT**。归档时知识上浮：用户可见规则 → `docs/reference/`，
> 机制与决策 → `docs/internals/`。

## 核心决定：零新概念

```
[ModuleInit] 方法们  ──编译期──>  <pkg>.$Module 伪类型的类型初始化器
                                   （挂既有 $Cctor 哨兵，载荷 = 合成函数 FQ 名）
```

全系统仍然只有**一个**初始化概念（类型初始化器）。`$Module` 与普通类型的唯一区别是
**谁来触发**：普通类型等访问点屏障（惰性），`$Module` 由**包加载收口点**主动触发（急切）。

复用既有哨兵 ⇒ **zbc / zpkg 格式不变，无 bump，无两代自举**。

## 硬约束：「加载那一刻同步回调」做不到

加载发生在 `lazy_loader` 的**写锁内**
（[lookup.rs:152-163](../../../../src/runtime/src/vm_context/lookup.rs)）。
初始化器是用户代码，一执行就会重入符号解析，而解析要抢同一把锁 ⇒ 死锁。
这条不是推测：`run_pending_static_inits` 的注释里已经记着
「**必须在 loader 锁释放后调用** —— 解析类型会触发包加载，而加载要抢同一把锁」。

**但这不影响 User 裁决的兑现。** 每条加载路径都已经有一个「锁已释放、刚加载的包名在手
（`newly_loaded`）、控制流还没回到用户代码」的收口点，而且**今天就已经在那里跑初始化器**。
所以能承诺的最强语义是：

> **zpkg 加载完成、加载锁释放后，本包任何代码被执行前**，包初始化器已执行完毕。

措辞与 CLR module initializer 一致。用户无法观测到它与「锁内同步执行」的差别——因为在这两点
之间，本包一行代码都还没跑。

## 触发点（运行期）：登记在加载漏斗，执行在屏障点

> ⚠️ **这一节在 IMPL 中相对初稿改过一次，理由见下。** 初稿写的是「在加载收口点
> （`try_lookup_*` 等 4 处锁后位置）执行」，实现时发现那里**抛不出可 catch 的异常**
> ——那些函数返回 `Option<Arc<..>>`，没有异常通道。

分工改成两段：

| 阶段 | 位置 | 做什么 |
|---|---|---|
| **登记** | `LazyLoader::insert_type`（每个加载进来的类型**都**流经的唯一漏斗，在加载写锁内） | 名字以 `.$Module` 结尾 ⇒ `CctorRegistry::register_module_init`。**只记账不执行**（锁内跑用户代码 = 死锁） |
| **执行** | 屏障点：静态调用 / `new` / 静态字段读写（interp ×3、JIT ×3） | `ensure_module_inits()` —— 有异常通道，失败能抛出可 `catch` 的类型初始化异常 |

**语义不变**：登记发生在解析该包符号的那一刻，执行发生在**同一次调用内、被调代码执行之前**。
例如 `Lib.Ping()`：解析 `Ping` → 加载 `lib` → 登记 → （锁已释放）→ 屏障 `ensure_module_inits`
→ 包初始化器跑完 → 才进入 `Ping` 的函数体。对用户仍然是「包加载后、本包任何代码跑之前」。

**主包**（不经惰性加载器）：`seed_types_for_lookup` 此前**直接写 `type_registry`、绕过
`insert_type`** ⇒ 主包自己的 `[ModuleInit]` 静默从不执行。现在那条路也做 `$Module` 检测，
第一个屏障点（`Main` 里的第一次调用 / `new` / 静态访问）把它跑掉。golden
`src/tests/module-init/runs_before_main` 守这条。

> 🔴 **只镜像 `$Module` 检测，不要把整个 seed 循环改走 `insert_type`。** 试过，回归立刻出现：
> 那样会给主模块每个类型的 cctor **再登记一次**（`boot.rs` 已经登记过），把屏障在启动期的
> 活跃集撑大 ⇒ 实测 `z42c build --workspace stdlib` 失败
> （`undefined function Std.IO.Environment.GetCommandLineArgs$0`）。收窄到只认伪类型后消失
> —— 这也是本次的退回对照证据。

> ⭐ **这个落法顺带堵上了纯惰性方案的洞**：cctor 屏障只有三处触发点（`obj_new` / 静态字段
> 读写 / 静态方法调用），而 `ensure_callee_owner_init` 靠「砍 FQ 名最后一段」推 owner
> 类型（[cctor.rs:421](../../../../src/runtime/src/vm_context/cctor.rs)）——**自由函数推不出
> owner，不触发任何初始化**。改成「包加载即触发」后，无论首次触达走类型还是自由函数，
> 都必然先经过加载收口点。

### 伪类型叫什么名字：`<ns>.$Module`，不含包名

⭐ **「一个包至多一个」这条裁决顺带解决了包名传递问题。**

`semantics` 层（`IrDump.BuildPackageCus` / `IrGen`）**拿不到包名** —— 包名只在 `pipeline`
层的 `req.PackageName`，而 `IrDump` 是自举冻结的跨包公开 API（"pipeline/driver 依赖的接口
冻结"），改签名要跨一个 nightly（[[bootstrap-seed]] 纪律）。

但伪类型的名字**不需要**包名：既然一个包至多一个 `[ModuleInit]`，就把 `<ns>.$Module`
发在**声明它的那个 CU 的命名空间**下，整包至多一个。运行期不必按名字猜包 ——
它扫刚加载模块的类型表，找名字以 `.$Module` 结尾的那一个。

⇒ 合成落在 **IrGen per-CU**（哪个 CU 有 `[ModuleInit]` 就在哪个 CU 发），
**零公开 API 变更、零包名传递、零跨 nightly 纪律**。包级只剩「至多一个」的校验。

### 「这个包有没有 `[ModuleInit]`」怎么判定

**不新增查表成本**：加载器在注册刚加载模块的类型表时已经逐个过一遍类型名，
判定「名字以 `.$Module` 结尾」就在那一遍里顺带记下（一个 `Option<String>` / 包）。

❌ **不要**在收口点按名去 `try_lookup_type` —— 没有 `[ModuleInit]` 的包会因此每次都撞一次
负缓存写锁；这是给 99% 的包付 1% 的成本。

> ⚠️ **这不是 #758 刚删掉的那种后缀扫描。** 那条删的是**加载期对全函数表**做
> `ends_with(".__static_init__")`（函数数量级，且是额外一趟）；这里是**类型表**
> （小一到两个数量级）且**搭在已有的注册那一遍上**，不新增遍历。差别要写进 internals 页，
> 否则下一个读代码的人会以为我们把刚删的东西加了回来。

### 成本预算

- 无 `[ModuleInit]` 的程序：`module_pending` 恒 0 ⇒ 每个屏障点一次 relaxed load
  （与旁边 `any_cctor_pending()` 同款 idiom，`cctor.rs` 文件头注释讲的就是这个手法）。
- 有 `[ModuleInit]`：每个包一次，走既有 `CctorRegistry` 状态机；**全部跑完后计数归零，
  屏障重新免费**。

### 失败语义靠「门不归零」实现

`refresh_module_pending` **只把 `Done` 算作完成**，`Failed` 不算 ⇒ 门保持非零 ⇒
之后**每一次**触达都会再走一遍屏障、再拿到那个 `Err`、再抛一次。这正是 spec 场景 5
要的「第二次触达仍抛」，而且不需要任何「函数 → 包」映射。

⚠️ 不能用「`ensure_type_init` 返回了 `Ok`」来判完成：初始化器内部再触发屏障时，
`claim` 对**同线程重入**直接放行并返回 `Ok`（C# 同款），那时初始化器其实还在跑。

## 执行顺序

1. **包内**：不存在顺序问题 —— **一个包至多一个 `[ModuleInit]`**（User 裁决）。
   ⭐ 这是「好的机制让错误写法无法表达」：不必写一条"别依赖包内顺序"的纪律再指望人遵守，
   也不必在编译期烘焙一个没人该依赖的定序。
2. **跨包**：**按实际加载顺序**（User 裁决）—— 不排序、不预扫。包 A 的 init 里触达包 B，
   B 的加载收口点先把 B 的 init 跑完，控制流再回到 A。这与 `unify-static-init-into-cctor`
   给出的「按真实依赖的拓扑序」是同一条性质，不引入第二套排序机制。
3. **循环**：A 的 init 触达 B、B 的 init 又触达 A —— 复用 cctor 的既有处理：同线程重入
   **放行**（可能看到部分初始化状态，C# 同款），跨线程**等待**到终态（`claim`，30s 上界）。

## 失败语义

`[ModuleInit]` 抛异常 ⇒ 复用 `CctorState::Failed`：该 `$Module` 标记失败，后续任何触发点
抛包装异常（对标 C# `TypeInitializationException`）。**不吞、不重试**。

## 编译期

### 识别

`[ModuleInit]` 的 kind 判定**并入 `attribute-handler-registry` 的三路判定**
（`HandlerRegistry.KindOf`），归 **Handler** —— 与 TIDX 同形（扫全 CU、聚合成包级产物），
而不是 Directive（那一路是"烘进 descriptor"的 `[Native]`/`[Deprecated]`/`[Record]`/`[Suppress]`）。
🔴 **不得**往那张被 PR1a 点名删掉的硬编码魔法名白名单里加名字。

`KindOf` 当前唯一的真消费点是 `AttributeSynth.z42:109`（`== StoreMeta` 才合成反射工厂），
所以归 Handler 的实际效果 = 豁免 `Attribute` 后缀约定 + 不合成死工厂。
`[ModuleInit]` 同时成为**保留名**，用户不可再定义同名 store-meta 类（同 `[Native]`）。

### 校验（新诊断码 **E0486** / **E0485**）

| 码 | 含义 | 报在哪 |
|---|---|---|
| **E0486** | 标注目标非法：必须是 `static` + 无参 + 返回 `void` + 非泛型 + 非抽象 | 该标注处（CU 内即可判定） |
| **E0485** | **一个包里出现了第二个 `[ModuleInit]`** | 后出现的那处，消息带上第一处的 `file:line` |

🔴 **两个码不得合并成一个** —— 「签名不合法」与「包内重复」是两件事，合并就是
[[diagnostic-code-uniqueness-program]] 刚归位过的一码两义。

**E0485 是包级判定**，CU 内看不到 ⇒ 判定点在包级汇总阶段（与 `$Module` 合成同一处）。
⚠️ 增量编译下只重编一个 CU 时，其它 CU 的 `[ModuleInit]` 信息必须从增量缓存拿得到，
否则重复会被漏报（判别力门见 tasks 4.4）。

> 🔴 **本变更自己就被抢号了一次**：`BadTarget` 初版取 E0484，rebase 到 main 时发现
> 「解引用标了 `?` 的字段/属性」已占走该号（那边早合入）⇒ 按先来后到让到 **E0486**；
> E0485 当时未被占，保留。这是同一形状的第四次（E0474 / E0477 / E0481 / 本次），
> 也再次印证 `xtask test diagcodes` 那道门的价值 —— 它在 rebase 的冲突里就把撞码顶了出来。
>
> 🔴 分配前必须 `grep -rn '"E0486"' src/`、`grep -rn '"E0485"' src/` 扫全源确认未被占用，并同步
> `DiagnosticCodes.z42`（唯一 SoT）+ `docs/reference/src/appendix/error-codes.md`。
> E0481 在 2026-09-22 被三个并行 PR 各拿了一次（#747/#759/#762），
> `xtask test diagcodes` 就是为此上线的——**合并前紧邻重查一次号**。

### 合成

两段分工（见上「伪类型叫什么名字」）：

| 阶段 | 位置 | 做什么 |
|---|---|---|
| **包级校验** | `IrDump.BuildPackageCus` 的包级扫描循环 | 扫全 CU 的 `[ModuleInit]`，**至多一个**，否则 E0485；诊断入 `coll.Diags`，按 `Span.File` 分发（`add-partial-types` 已铺好这条路） |
| **合成** | `IrGen` per-CU | 本 CU 有 `[ModuleInit]` ⇒ 发 `<ns>.$Module` 类型 + 其类型初始化器（`call` 那一个方法，`ret void`），挂 `$Cctor` 哨兵 |

⚠️ CU 是**并行**编译的（`CompileCuTask` / `ParallelFor`），所以合成必须是 per-CU 纯函数式的
（只看本 CU），不能跨 CU 共享可变状态 —— 这正是「校验在包级、合成在 CU 级」的分工理由。

## 用户纪律（写进 reference）

1. **一个包只能有一个 `[ModuleInit]`** —— 多处装配就在那一个方法里按需要的顺序写出来。
   （编译器强制，不是建议。）
2. **不要依赖跨包 init 的相对顺序** —— 它就是实际加载顺序，换一行调用就可能变。
3. **`[ModuleInit]` 不是「注册」的最优解**。如果要做的是「把一批东西登记进表」，
   编译期收集更好：z42 已有 `TIDX` 这个形状（运行期直接读表，没有谁跑回调来注册自己）。
   `[ModuleInit]` 适合**真正的运行期装配**（打开 native 库、读环境、建连接池）。

## 将来：什么时候该升级到「接口 + 清单指名」

横向调研（已做，别重做）：**没有哪个长期演化的系统是靠「每加一个钩子就加一个 attribute」
活下来的** —— OSGi `Bundle-Activator`、Erlang/OTP `.app` 的 `{mod,...}`、
androidx.startup `Initializer<T>` 全都收敛到「一个接口 + 元数据指名实现类」。

**升级判据（写死在这，避免将来靠感觉决定）**：当包级回调种类 **≥ 3**（例如再来
`[ModuleShutdown]` + `[ModuleReload]`），或某个回调需要**声明式依赖 / 参数**时，
收敛为 `IModuleLifecycle` 接口 + `packages.toml` 指名实现类。在那之前，多一个 attribute
比多一套解析路径便宜。

## 已知差距：包初始化失败的异常当前不可 `catch`

**实测（2026-09-23）**：`[ModuleInit]` 抛异常 ⇒ 正确地包装成
`Std.TypeInitializationException` 抛出、失败状态被记住、程序终止 —— 但用户的 `try/catch`
**抓不到它**。

已用干净 A/B 排除的因素（每条都单独跑过，见下「怎么查的」）：

| 变量 | 结果 |
|---|---|
| 调用形态：跨包自由函数 vs 跨包静态方法 | 都不可捕获 |
| `catch (TypeInitializationException)` vs 无类型 `catch {}` | 都不可捕获 |
| 执行模式：jit vs `Z42_MODE=interp` | 都不可捕获 |
| 屏障插入位置：`exec_call` 独立一行 vs 挪进 `ensure_callee_owner_init` 内部 | 都不可捕获 |
| **对照组**：跨包类型 cctor 抛（直抛 / 嵌套帧抛）、单包 cctor 嵌套帧抛 | **都能正常捕获** |

⇒ 差别收敛到一点：**只有包初始化这条路，抛出发生在「这次调用内同时发生了包加载」之后**。
根因未查清。

**为什么不留一条红着的门**：一个守着不成立行为的 fixture 只会让 GREEN 长期红着，
而 [[fake-gate-lets-compiler-bug-into-main]] 的教训是「会红但不挡人的门 = 没有门」。
所以本变更**不为这条写 e2e**，改为把差距显式登记在这里与 spec 里。

**影响面**：用户无法 `catch` 包初始化失败并继续。鉴于包装配失败后包内一切都不可信，
「终止」本身是可接受的落点（C++ static init fiasco 一族的通行处理），但它与 z42 既有的
「类型初始化失败可 catch」不一致 —— 这条不一致是真的，应单独立项查。

### 怎么查的（复现配方，供接手的人直接用）

1. 建两个 cross-zpkg fixture，`main` 逐字同形，只换依赖包：一个包用 `[ModuleInit]` 抛，
   另一个用类的 `static C() { Boom(); }` 抛。
2. `./xtask test e2e --dir cross-zpkg`，对比两者的 actual。
3. ⚠️ **fixture 里被调用的函数必须有真实函数体** —— 空函数 `public void Ping() { }` 会被
   内联掉，连 call 指令都不发射，屏障无从触发，看起来像「try 抓不到」，实则异常推迟到了
   下一条调用。第一版 fixture 正是栽在这里。
4. ⚠️ **别用 `eprintln!` 探针**：stderr 与 z42 的 stdout 交错，e2e 比较的是合并流，
   顺序不可信；换 `println!` 又会挤进 actual 把测试本身搅乱。这条路上浪费了两轮。

## 备选方案（已否）

| 方案 | 否掉的理由 |
|---|---|
| 包内多个 `[ModuleInit]` + 编译期定序 | User 裁决取「至多一个」：定序要么没人该依赖（那就别提供），要么有人依赖（那就是个隐式契约）。禁掉是更小的机制 |
| 纯惰性（CLR 式，首次触达本包类型才跑） | 自由函数路径不触发（见上）⇒ 用户看到「注册表是空的」而非报错；User 已裁决取加载即执行 |
| 编译期烘焙 + 入口前必跑（依赖闭包拓扑序） | 语义最强，但动态加载（REPL / 反射加载 zpkg）的包不经此路，要两套机制；且强制启动加载全部含 init 的包，与 #418 取向相悖 |
| 惰性默认 + 清单 `eager = true` | 两条语义并存 = 又回到「一个概念两套机制」，与刚做完的收敛方向相反 |
| 新关键字（`module init { }`） | 既有决定 D1/D7：声明位一律 `[X]`，不引入新关键字 |
