# Proposal: 静态初始化统一到类型初始化器（cctor）

> 状态：DRAFT（2026-09-22 起草）｜类型：`lang`（可观察语义变更）+ `vm`｜完整流程
> 后继：`add-module-init-hook`（包级 `[ModuleInit]`）建在本变更之上，本变更不含它。

## Why

z42 现在有**两套**静态初始化机制，做同一件事：

| 机制 | 粒度 | 时机 | 谁在用 |
|---|---|---|---|
| `<ns>.<stem>.__static_init__` | **每文件** | 包加载后批量跑 | 没写静态构造器的类（= 全仓 519 条初始化器的全部） |
| 类型初始化器（`$Cctor` 哨兵） | 每类型 | 首次使用该类型前 | 全仓 **0 处** |

两套并存的代价是真金白银：

1. **编译器有一个二分支**：同一个静态字段初始化器，按"宿主类有没有写 `static C()`"分别送进
   per-CU `__static_init__` 或 cctor 体首（[DeclBinder.z42:180-192](../../../../src/compiler/z42c.semantics/src/DeclBinder.z42)）。
   这个分支自己就产过 bug——注释里记着"cctor 写了 42、随后 `__static_init__` 又覆写回 1"。
2. **运行期有一整条平行管道**：`pending_static_inits` 队列 + `InitState::Claimed` 认领状态机 +
   `running_static_inits` 计数 + `init_batch_inflight` 在飞计数，与 cctor 自己的
   `CctorRegistry` 状态机**各跑各的**，`run_pending_static_inits` 要同时排空两个队列
   （[lookup.rs:265-325](../../../../src/runtime/src/vm_context/lookup.rs)）。
3. **加载期有一次全函数表字符串扫描**：每加载一个包，对**每个函数名**做
   `ends_with(".__static_init__")`（[registry.rs:72](../../../../src/runtime/src/metadata/lazy_loader/registry.rs)）。
4. **语义要多解释一段**：reference 里必须警告"给一个类加静态构造器，会把它的静态字段初始化
   从启动时变成首次使用时"——这条用户可见的行为悬崖，正是两套机制的接缝。

5. **而且它今天就在静默给错值。** 实测（2026-09-22，当前 main 自建工具链）：

   ```z42
   class A { public static int X = B.Y + 1; }   // 声明在前
   class B { public static int Y = 41; }        // 声明在后
   → A.X = 1        // 期望 42；引用类型版本得到 null，无任何报错
   ```

   把 B 挪到前面 → 42。**给 B 加一个空壳 `static B()`（改走类型初始化器）→ 也是 42。**
   即：要统一到的那条管道行为本来就对，要删掉的那条是错的。本变更因此不只是简化，
   **同时修掉一个现存的静默正确性缺陷**（形状见 [[silent-feature-masks-other-bugs]]）。

**而第二套机制没有语义必要性。** z42 顶层只允许 `class`/`struct`/`interface`/`enum`/`delegate`/
`impl`/自由函数，**没有顶层变量**（[Parser.z42:400-417](../../../../src/libraries/z42c.syntax/src/Parser.z42)）；
`SemanticModel.AddStaticInit(cls, field, init)` 的**每一条都带宿主类名**
（[DeclBinder.z42:191](../../../../src/compiler/z42c.semantics/src/DeclBinder.z42) / `:236`）。
`__static_init__` 里一条"无主"条目都没有——它纯粹是按文件聚合的实现细节。

## What Changes

**静态字段 / 静态 auto 属性的初始化器，一律成为其宿主类型的类型初始化器的一部分。**
`__static_init__` 这个概念从编译器、zbc/zpkg、运行期**整体删除**。

全系统此后只有**一个**静态初始化概念：**类型初始化器，在该类型首次使用前执行**。
四个触发点不变（读/写其静态字段、`new`、调其静态方法）。

### 语义变更（需 User 明确批准）

**现状**：没写 `static C()` 的类，其静态字段初始化器在**所属包加载后批量执行**。
**变更后**：改为**该类型首次使用前**执行。

后果，逐条列明：

- **执行时机后移**：一个被加载但从未使用的类，其静态字段初始化器**不再执行**。
  对标 CLR/JVM（`beforefieldinit` 类型的初始化时机由运行时自由选择，JVM `<clinit>` 只在主动使用时跑）。
- **跨类型的相对顺序不再有隐含保证**：现状同一文件内所有类的初始化器按声明序在一个函数里连续执行。
  变更后各类型独立惰性触发，相互顺序由使用顺序决定。
  **实施上**仍按原声明序注册，但**规范上不承诺**（对齐 CLR/JVM）。
- **`static readonly` 的可观测值不变**——首次读即触发初始化，读到的永远是初始化后的值。
- **失败语义统一**：初始化器抛异常 → 该类型标记 `Failed` → 后续访问一律
  `TypeInitializationException`。现状 `__static_init__` 抛出是 `bail!("uncaught exception in static init")`
  直接打死进程（[entry.rs:103-106](../../../../src/runtime/src/interp/entry.rs)），**变更后成为可捕获的类型初始化失败**。这是严格的改善。

### 编译期屏障消除（必做）

运行期再便宜，也不如**根本不发射**。编译器在**整包装配之后**为每个静态访问 / `new` /
静态调用站点置一个**正向位 `owner_init_free`**——「编译期证明该站点的 owner 类型没有类型
初始化器」。置位 ⇒ 不发屏障；**缺席 ⇒ 走完整运行期检查（= 今天行为）**。

绝大多数类根本没有静态字段，因此绝大多数站点可证 init-free，屏障代码压根不存在。

形状直接对标已在仓库里的 `CtorKnownFixup`（zbc 1.39），三条纪律一并移植：**必须等到整包
装配之后**（发射那一刻本 CU 看不到同包其它文件、`Deps` 不含本包）、**缺席即保守态**、
**每次装配重算全部站点**（增量安全）。

**跨包站点一律不置位**：编译时依赖 v1 的 `C` 无初始化器 → 置免检 → 运行时装到 v2 的 `C`
有初始化器 ⇒ 静默跳过初始化。不给 dep-version-skew 开新入口。

### 屏障实现改造（必做，非可选优化）

现状热路径门 `CctorRegistry::any_pending()` 之所以"几乎免费"，前提是**全仓没人用 cctor**，
计数恒 0。本变更把 519 条初始化器的宿主类全部纳入，**这个前提立即失效**：门非零时
每次静态访问要走 `owner_class_of_static_field()` 切字符串 → `type_registry` 哈希查表 →
`CctorRegistry::map` 上锁再查一次表（[cctor.rs:204-238](../../../../src/runtime/src/vm_context/cctor.rs)）。
短程序里很多类型一辈子不被触达，门**整个进程关不上**。

因此状态必须从「全局 `Mutex<FxHashMap<String, CctorEntry>>` + 计数门」搬到
**`TypeDesc` 上的一个 `AtomicU8`**：屏障退化为一次 relaxed load，与今天等价，且
**不再依赖"没人用 cctor"这个脆弱前提**。`ObjNew` 早已是这个形状（手上有 `TypeDesc`，
`td.cctor_func()` 一次 `Option` 判断），本变更把另外三个触发点对齐过去。CLR/JVM 同样把
初始化标志放在方法表/类元数据里，而非全局注册表。

### 格式

**零 bump。** `$Cctor` 是类级 attr-ref 哨兵（零格式-bump 机制，先例 `$Deprecated`），
本变更只是让更多类挂上它。zpkg 0.49 / zbc 1.44 不动（版本以 `ZpkgWriter.Minor` 为准，2026-09-22 核）。

## Scope（允许改动的文件）

### compiler

| 文件 | 变更 | 说明 |
|---|---|---|
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | MODIFY | 删二分支：字段/静态 auto 属性初始化器一律注入宿主类型初始化器 |
| `src/compiler/z42c.semantics/src/SemanticModel.z42` | MODIFY | `SiCls/SiField/SiInit` 改按宿主类分组供合成消费 |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | 删 `EmitStaticInit`；无显式 cctor 但有初始化器的类改走合成类型初始化器 |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | 删 "static_init 首位" 特判与 `SourceStem` 依赖 |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | 删 SA-3 `SourceStem` 设置 |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | `$Cctor` 哨兵挂载条件扩为「有显式 cctor **或** 有静态初始化器」 |
| `src/compiler/z42c.semantics/src/AccessEmitter.z42` | MODIFY | 静态 struct 字段装箱统一走 `_emitStaticStore`（删 `EmitStaticInit` 专用转发） |
| `src/compiler/z42c.pipeline/src/CtorKnownFixup.z42` | MODIFY | 整包装配后的置位遍历中并入 `owner_init_free`（或新增同形 pass） |
| `src/compiler/z42c.pipeline/src/PackageCompile.z42` | MODIFY | 装配点调用新置位逻辑 |
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | IR 指令承载 `owner_init_free` 位 + zbc 往返 |

### runtime

| 文件 | 变更 | 说明 |
|---|---|---|
| `src/runtime/src/metadata/types/type_desc.rs` | MODIFY | `TypeDesc` 增 `init_state: AtomicU8` |
| `src/runtime/src/vm_context/cctor.rs` | MODIFY | 状态机搬到 `TypeDesc`；删全局计数门；保留 `Failed` 文案表 |
| `src/runtime/src/vm_context/statics.rs` | MODIFY | 静态读写屏障改用 owner `TypeDesc`；删名字回灌逻辑 |
| `src/runtime/src/vm_context/lookup.rs` | MODIFY | `run_pending_static_inits` 塌缩为单队列 |
| `src/runtime/src/vm_context/types.rs` | MODIFY | 删 `pending_static_inits` 相关计数字段 |
| `src/runtime/src/metadata/lazy_loader.rs` | MODIFY | 删 `pending_static_inits` / `static_init_state` |
| `src/runtime/src/metadata/lazy_loader/registry.rs` | MODIFY | 删后缀扫描；改为登记「有 `$Cctor` 哨兵的类型」 |
| `src/runtime/src/metadata/lazy_loader/resolve.rs` | MODIFY | 删 `InitState::Claimed` 窗口逻辑 |
| `src/runtime/src/metadata/loader/type_registry.rs` | MODIFY | 登记点统一（急切 + 惰性同一漏斗） |
| `src/runtime/src/metadata/resolver.rs` | MODIFY | 排空点调整 |
| `src/runtime/src/metadata/well_known_names.rs` | MODIFY | 删 `METHOD_STATIC_INIT` |
| `src/runtime/src/interp/entry.rs` | MODIFY | `init_static_fields` 塌缩 |
| `src/runtime/src/jit/mod.rs` | MODIFY | 镜像塌缩；删 `collect_lazy_static_init_names` |
| `src/runtime/src/host/ops.rs` / `host/state.rs` | MODIFY | 宿主加载路径同步 |
| `src/runtime/src/boot.rs` | MODIFY | 启动步骤表更新 |

### docs

| 文件 | 变更 |
|---|---|
| `docs/reference/src/language/static-constructors.md` | MODIFY — 统一叙述；删「加 cctor 会改变时机」的行为悬崖警告 |
| `docs/reference/src/language/static-members.md` | MODIFY — 静态字段初始化时机 |
| `docs/internals/src/runtime/static-ctor-init.md` | MODIFY — 屏障从全局门改为 per-TypeDesc；删双管道叙述 |

## Out of Scope

- **`[ModuleInit]` / 包级回调**——独立后继变更 `add-module-init-hook`。
- **格式 bump**（本变更零 bump）。
- **`EmitStaticInit` 的「不带 DBUG 行表」约定**：该约定的理由是与已移除的 C# bootstrap 编译器字节对齐，
  随 `EmitStaticInit` 一并消失，不单独立项。
- 惰性加载的触发点集合（T1/T2/T3）本身不动。

## GREEN 判据

- `xtask test` 全绿（interp）+ `xtask test stdlib --mode jit`（屏障改在派发面，必跑）。
- `cargo test`（**全量，非只 `--lib`**）全绿。
- 跨包用例 `src/tests/cross-zpkg/static_init_cross_pkg` / `static_init_concurrent` 不回归。
- **新增负例门**：一个静态字段初始化器抛异常的 fixture，断言得到可捕获的
  `TypeInitializationException` 而非进程 `bail!`。
- **性能对账**：`z42c` 自举编译 + hello 启动的 A/B；编译器类改动被 bench 门判红时按
  [[investigate-micro-ab-false-regressions]] 做字节对账证伪。
- **顺序回归扫描**：全仓 519 条静态初始化器中，跨类型顺序依赖逐条排查（见 tasks.md 阶段 7.0）。

## Open Questions

1. 跨类型顺序：规范上明确"不承诺"（本 proposal 的取态），还是承诺"同文件内按声明序"？
2. `TypeDesc.init_state` 的失败文案：继续放全局表（少见路径），还是随 `TypeDesc` 冷区走？
