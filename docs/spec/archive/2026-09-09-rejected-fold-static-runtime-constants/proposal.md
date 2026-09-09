# ❌ REJECTED: 静态运行期常量的自动推断与折叠

> **状态：🔴 未实施，已否决（2026-09-09）。零代码落地。**
> 保留本文档的唯一目的：**记录四轮被否决的方案与实测数据，避免有人重走一遍。**
>
> ## 否决理由（按发现顺序）
>
> **① 价值论证建立在测错的对象上。** 最初用「调 native 桩 56ns vs 常量 3ns → 省 52ns」
> 论证收益。但本设计折的是 **`StaticGet`（读静态字段）**，不是 native 调用——把值存进静态
> 字段后，native 调用**只在静态初始化时发生一次**，根本不在热路径上。重测同一条真实路径：
>
> | ps/iter | JIT | interp |
> |---|---|---|
> | `StaticGet` | 8.9 ns | 19.2 ns |
> | 常量 | 4.0 ns | 14.4 ns |
> | **折叠真实收益** | **≈4.8 ns** | **≈4.7 ns** |
>
> 比原论证**小 10 倍**。教训：**量测必须量你真正要改的那条路径**；测了相邻的东西再套用，
> 比不测更危险——你会带着数字的信心去做错事。
>
> **② 有不可撤销的正确性风险。** `static_fields_clear()`（`vm_context/statics.rs`）在**每次
> entry-point 运行前**把所有静态字段清零并重排初始化器。多 entry 宿主（z42b test runner /
> REPL / 嵌入）会重跑初始化，值可能不同。而 JIT 一旦把常量烘进机器码就**撤不回来**——VM
> 没有代码失效（invalidation）机制。这会在 test host 上产生**静默错误答案**。
>
> **③ 有更好的替代。** 那 4.8ns 的来源已查明：`static_get_by_id` **每次读都加锁**
> （`static_fields.lock()`）+ 一次 `Value` clone。直接优化这条读路径（去锁）比「推断哪些字段
> 是常量再折掉它们」更好：收益同等或更大、覆盖**所有**静态字段、无推断 pass、**无烘常量的
> 失效风险**。
>
> **④ 但连替代方案也未必值得做。** 4.8ns 是否重要，必须先量真实负载（如 z42c 自编译里
> `StaticGet` 的占比），不能靠 microbench 定夺。此项未做 → 整条性能线搁置。
>
> ## 本文档下方保留的内容
>
> 「设计演进」一节记录了**另外三个**被否决的方案（`[Invariant]` attribute / 复用 `readonly`
> 修饰符 / 本方案），以及一个重要的事实勘察：`CallNativeInstr` 全编译器**只有一个生产点**
> （`StubEmitter`），只存在于 `[Native]` 桩函数体内 → 「折叠让含 CallNative 的函数变得可 JIT」
> 这个曾被当作最大收益的说法**不成立**（调用方持普通 `Call`，本来就可 JIT）。
>
> ## 实际落地的是什么
>
> 本轮讨论真正产出的是**两个 bug 修复**（与性能无关，价值独立）：
> - `static readonly` 不被强制 → `fix-static-readonly-not-enforced`（本 PR）
> - 静态构造函数静默不执行 → 独立立项

---

> **状态：DRAFT，待 User 确认。** 2026-09-09
> **Supersedes `add-invariant-attribute`**（该 DRAFT 已作废，未落任何代码）——设计经过三轮
> 修正后，从「用户断言 + 新 attribute + 跨两个 nightly」收敛成「VM 自证 + 零语法 + 单 PR」。
> 作废理由见下「设计演进」。

## Why

进程内恒定的值，每次读都付全额调用代价。实测（3M 次循环，三次一致）：

| | JIT | interp |
|---|---|---|
| 调 `[Native]` 桩（`Platform.OSKindValue()`） | **56 ns/iter** | 78 ns/iter |
| 调普通 z42 函数 | 4 ns/iter | 13 ns/iter |
| 直接用常量 | 3 ns/iter | 13 ns/iter |
| **折叠收益** | **52 ns（省 94%）** | **64 ns（省 83%）** |

普通 z42 调用在 JIT 下几乎免费（4 vs 3 ns）——**52ns 几乎全是 native 往返开销**
（`jit_call` → `jit_builtin` helper → 每次一次 `Vec<Value>` 堆分配 → `BUILTINS[idx]`）。

> 精度声明：`__platform_os_kind` 走 **`Builtin`** 路径。真正的 libffi `CallNative`
> （`[Native(lib=,entry=)]`）更贵——JIT 直接拒编整个桩函数。故 52ns 是**下界**。

而典型写法本来就把这类值存进静态字段：

```z42
public static int DeviceCount = NativeGetDeviceCount();   // 调用只发生一次
```

`__static_init__` 跑完之后，这个字段**再也不会变**。今天每次读它仍走 `StaticGet` helper，
且 `StaticGet` 被编译器保守判为有副作用（「可能触发静态初始化」）而不参与任何优化。

## What Changes

**VM 加载期自动推断 + 折叠。零新语法、零新 attribute、零用户改动。**

- 加载期扫描：某静态字段除了 `__static_init__` 之外，**是否还有别的 `StaticSet` 写它**。
- 没有 → 该字段在静态初始化完成后是**运行期常量**。
- 其 `StaticGet` 站点在静态初始化完成后可折成常量（interp 与 JIT 共享）。

用户**什么都不用写**：

```z42
public static int DeviceCount = NativeGetDeviceCount();
if (DeviceCount > 1) { ... }     // ← 折成常量，热路径零调用
```

## 为什么这是可自证的，不是断言

折叠的是**已被捕获的值**，不是调用：

- 折 `StaticGet X` 的正确性**完全不依赖 native 返回什么**——哪怕 `NativeGetDeviceCount()`
  每次调都返回不同值也无所谓，它**只被调了一次**，字段捕获了那一次的结果。
- 唯一的破坏者是「初始化后还有人写这个字段」。VM 有**全程序视图**，扫一遍 `StaticSet` 即可证明。

**反射写不了静态字段**（已实测）：`builtin_field_set_value`
（[`accessors.rs:175`](../../../../src/runtime/src/corelib/reflection/accessors.rs)）
只处理 `BoxedStruct` / `Object` 两种 target，静态字段写入落到
`_ => bail!("target is not an object instance")`。故没有反射后门。

> ⚠️ 这是**当前事实**，不是永久保证。故本 change 必须留一个**会变红的哨兵**（见 design D4）：
> 谁将来给 `FieldInfo.SetValue` 加静态字段支持，那个测试立刻红，逼他同时处理折叠假设——
> 而不是静默产生错误答案。

## 设计演进（三轮修正，记录以免重走）

| 轮次 | 方案 | 为什么否决 |
|---|---|---|
| 1 | `[Invariant]` attribute 标 native 函数，首次调用折叠 | 不可验证的用户断言；标错 = 静默错误答案；新 directive attr → 跨两个 nightly。且原以为的「让含 CallNative 的函数变得可 JIT」二阶收益**经勘察不存在**——`CallNativeInstr` 全编译器只有一个生产点（`StubEmitter`），只存在于桩函数体内，调用方本来就可 JIT |
| 2 | 复用 `readonly` 修饰符标函数 | `readonly` 今天的语义是「存储位置不可再赋值」且**被强制检查**。挂到函数上会变成「一处编译器强制、一处完全不查」——用户从字段学到的直觉会害了他。且与 C# 式 `readonly` 成员、ref/borrow 的 `ref readonly` 返回位撞车 |
| 3 | **本方案**：VM 自动推断静态字段 | 零语法、零断言、VM 自证、单 PR |

关键转折点是认识到：**「折调用」需要不可证的断言，「折字段」只需要可证的事实**，而后者覆盖了
绝大多数真实场景（值在进程启动时取一次就够：设备能力、平台信息、驱动版本、配置）。

## Scope（允许改动的文件）

### 运行时（Rust VM）
- `src/runtime/src/metadata/loader/` — 新增静态常量推断 pass（扫 `StaticSet` 写点）
- `src/runtime/src/vm_context/statics.rs` — 常量静态字段的读路径
- `src/runtime/src/interp/` — `StaticGet` 命中常量 → 直接取值
- `src/runtime/src/jit/translate/object.rs` — JIT 侧烘常量（若静态初始化已完成）
- 哨兵测试（反射写不了静态字段）

### 测试 / 文档
- `src/tests/optimization/` — 折叠正确性 + 可观测性用例
- `src/runtime/src/.../*_tests.rs` — 推断 pass 单测 + 反射哨兵
- `docs/book/src/runtime/` — 机制页

### 只读引用
- `src/compiler/z42c.semantics/src/FunctionEmitter.z42`（`__static_init__` 合成）

## Out of Scope

- **折叠 native 调用本身**（原 `[Invariant]`）——明确**不做**，理由：不可验证、错了静默。
  若将来出现「必须在热路径直接调且确知不变」的实证需求，再单独立项，且必须配 debug 复检。
- **`static readonly` 强制**——独立 bug（实测 `C.A = 99;` 静默放行），单独立项。
  它是**契约**问题，与本 change 的**优化**正交。
- **静态构造函数不执行**——独立 bug（实测 `static C() {}` 函数体静默不跑），单独立项。
- **惰性静态初始化**（首次用到才跑初始化器）——正交特性，若「native 库在静态初始化时还没
  dlopen」成为实证问题再评估。

## Open Questions（需 User 裁决）

1. **折叠的值类型范围**：v1 限标量（`int/long/bool/char/f64`）还是也含 `string`？
   引用类型（数组/对象）**一律排除**——字段虽不再被重新赋值，但对象内部可变。
   我倾向 v1 只标量，`string` 因不可变可含但要确认 GC 根处理。
2. **静态初始化完成的判定**：VM 如何知道「`__static_init__` 已跑完」？逐模块标记，还是全局
   一次性？涉及跨包惰性加载（新模块加载会带来新的静态初始化）。这是本 change 的**主要技术风险**。
