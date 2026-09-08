# Proposal: 静态构造函数（C# 语义：惰性、按类型、首次使用前）

> **状态：DRAFT，待 User 确认。** 2026-09-09
> User 已裁决语义方向：**按 C# 语义**（惰性、per-type、首次使用前），而非「跟着模块静态
> 初始化一起跑」。本文档要摆清的是：这个方向有一处**与现有 VM 架构的真实冲突**，需要你
> 在「语义保真度」和「热路径性能」之间做第二次裁决。

## Why —— 这是个静默失效的语言特性

```z42
class C {
    public static int V = 1;
    static C() { Console.WriteLine("ran"); C.V = 42; }
}
void Main() { Console.WriteLine(C.V); }   // 实测输出 `1`，且没有 "ran"
```

**编译通过、运行不报错、构造器体就是不跑。** 用户看到的是一个「看起来支持、实际静默失效」
的特性——比根本不支持危险得多。

实测确认它**不是没被编译**：差分对照（同一个体，一个写成 `static C()`、一个写成
`static void Init()`）产出 zbc 523 B vs 526 B（差 3 B = 名字长度），而空类只有 394 B。
⟹ **静态构造器被完整编译并发射成了函数，只是从来没人调用它。**

编译器全程知道它的存在——`DeclBinder` 里到处是 `md.IsCtor && !ms.IsStatic` 这样的排除判断
（`_hasExplicitInstanceCtor` / `inCtor` / 字段初始化器注入），即「静态 ctor 走另一条路」。
但那条路**不存在**。

## What Changes

让静态构造器按 C# 语义执行：

1. **至多执行一次**（每类型、每进程），线程安全。
2. **在首次使用该类型之前**执行——触发点：创建实例（`ObjNew`）、读写其静态字段
   （`StaticGet`/`StaticSet`）、调用其静态方法（`Call`）。
3. 重入（A 的 cctor 触发 B、B 又触发 A）不死锁：沿用 C# 的处理——检测到本线程已在跑，
   直接放行（可能看到部分初始化的状态）。

## ⚠️ 与现有架构的真实冲突（需 User 第二次裁决）

C# 的 cctor 屏障之所以「免费」，是因为 **JIT 在类型初始化完成后把检查从机器码里patch 掉**。
**z42 的 JIT 没有代码 patch / 失效机制**（`FnEntry` 一旦 `OnceLock::set` 就永久有效，
无 deopt、无重编译）。

于是「首次使用前」的屏障只能是**每次访问都检查**：

| 方案 | 保真度 | 代价 |
|---|---|---|
| **A. 每次访问都查初始化状态** | C# 精确 | `StaticGet` 实测已是 8.9 ns（JIT），再加一次状态查 + 分支 → **所有静态字段读都变慢**，包括从不涉及 cctor 的绝大多数 |
| **B. 只在有 cctor 的类型上插屏障** | C# 精确（对有 cctor 的类型） | 只惩罚有 cctor 的类型；需要 emit 期知道「该类型有 cctor」并发不同的指令/标记 |
| **C. 复用现有 per-CU 静态初始化时机**（cctor 跟着 `__static_init__` 跑） | **不是 C# 语义**（非惰性：没用到的类也会跑） | 零热路径代价，改动最小 |

**我推荐 B**：保真度与 A 相同，但代价只落在真正有静态构造器的类型上——而那是少数。
代价是需要一个「此类型有 cctor」的编译期标记传到运行期（可走既有的 `class_flags` 或
attr-ref 哨兵通道）。

> ⚠️ `class_flags` 是 u8 且**已用满 8 位**（`bytecode.rs` 注释直言 "Last free class_flags bit"），
> 所以要么走 attr-ref 哨兵（零格式 bump，先例 `$Deprecated`），要么 bump 格式。倾向哨兵。

**如果你要的是「不惜代价的 C# 精确」，选 A；如果接受「只惩罚有 cctor 的类」，选 B；
如果性能优先且能接受非惰性，选 C。** 我建议 B。

## 现有可复用的设施（好消息）

不是从零开始。`vm_context/lookup.rs` 已有一套**成熟的**静态初始化状态机：

- `InitState { Claimed(tid) / Running(tid) / Done }` —— 含跨线程等待与**重入放行**
- `static_init_state: HashMap<String, InitState>` —— 目前键是 **per-CU 的 `__static_init__` 函数名**
- `run_pending_static_inits()` —— 排空循环，含 `DRAINING` 防嵌套、`init_batch_inflight` 静止判定
- `enqueue_type_init(class_fq)` —— 已按**类 FQ** 排队（但目前只有一个调用点，在 token 解析期，
  用途是触发所属包加载，不是「首次使用屏障」）

**本 change 主要是把这套 per-CU 的机器扩成 per-type，并接上真正的触发点**，而不是新造状态机。
并发正确性可以直接沿用已被实践检验的那套（那块代码有明确的 `Claimed`/`Running`/重入注释，
显然是踩过坑写出来的）。

## Scope（允许改动的文件）

### 编译器
- `src/compiler/z42c.semantics/src/MemberCollector.z42` / `DeclBinder.z42` — 识别静态 ctor，
  登记「此类型有 cctor」（方案 B）
- `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` — `$Cctor` 哨兵（若走哨兵通道）
- `src/compiler/z42c.semantics/src/AssignTyper.z42` — **必须回改**：放行静态 ctor 内对
  `static readonly` 的赋值（见「与 PR #544 的耦合」）

### 运行时
- `src/runtime/src/vm_context/lookup.rs` — per-type init state（扩现有机器）
- `src/runtime/src/interp/exec_object.rs` / `exec_call.rs` — 触发点屏障
- `src/runtime/src/jit/helpers/` — 同上（JIT 侧）
- `src/runtime/src/metadata/` — 读 `$Cctor` 标记

### 测试 / 文档
- `src/tests/` — 执行时机 / 一次性 / 重入 / 并发 / 异常
- `docs/book/src/language/` — 静态构造函数页

## Out of Scope

- **`TypeInitializationException` 包装**（C# 在 cctor 抛出后把类型标记为不可用，后续访问抛
  包装异常）——v1 先让异常直接传播，包装语义单独评估。
- **泛型类型的 per-实例化 cctor**（C# 里 `C<int>` 和 `C<string>` 各跑一次）——v1 先按
  开放类型一次，若需要再扩。

## 与 PR #544 的耦合（必须一起处理）

PR #544（`static readonly` 强制）在 `AssignTyper._checkReadonlyAssign` 里对静态 readonly
**一律报错**，因为「z42 今天静态 ctor 体根本不跑」。那里已经写明：

> ⚠️ 修好静态 ctor 时**必须回到这里**：给 TypeEnv 加 `InStaticCtor` 并在此放行，
> 否则合法的静态 ctor 赋值会被误报。

本 change 必须包含这一步，否则 C# 里最常见的写法（在静态 ctor 里给 `static readonly` 赋值）
在 z42 里会被拒。**这是 #544 合并后本 change 的硬前置依赖。**

## Open Questions（需 User 裁决）

1. **屏障方案 A / B / C**（见上）。我推荐 **B**。
2. **触发点是否含「调用静态方法」**？C# 含。但那意味着每个静态 `Call` 也要过屏障（方案 B 下
   只影响有 cctor 的类）。若只做「实例创建 + 静态字段访问」会偏离 C#，但覆盖绝大多数场景。
3. **cctor 抛异常的语义**：v1 直接传播（简单），还是立刻做 C# 的「类型标记为失败 + 后续访问
   抛包装异常」？我倾向 v1 直接传播，把包装留给后续。
