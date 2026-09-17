# 设计哲学

> 对齐：2026-09-17 ｜ 代码：本页无单一实现物；每节末尾指向承载该决定的实现页
>
> 待办：`T?` 是被擦除的纯注解（**当前没有空安全**）、`async`/`await` 只有 token 没有语义、
> hot reload 与 AOT 只有设计页——完整清单见文末[已知边界](#已知边界)。

这页是 z42 的**设计北极星**：语言为谁设计、往哪演进、哪些决定永不改变。
改 z42 时拿不准「该不该加这个特性」「这样实现对不对」，先读这里再动手。

本页**只写取舍与理由**，不写机制。机制在各子系统页；用户视角的规则在[语言参考](../../reference/src/language/README.md)。

---

## 一、为谁设计

z42 面向**系统方向的应用开发者**——同一门语言从嵌入式固件写到云端后端，不用中途换语言。

四条诉求，缺一不可：

| 诉求 | 落点 |
|---|---|
| **全栈覆盖** | 一份字节码，interp / JIT / AOT 三档执行；VM 可嵌入宿主 |
| **性能** | native 互操作零中转、可预测的对象布局、JIT 热路径 |
| **开发速度** | C# 系语法、强类型 + 局部推断、编译即跑 |
| **可上生产** | 多执行模式、可观测、GC |

**明确不争取的人群**：纯脚本场景（用 Python）、必须消除 GC 的场景（用 Rust）、
已有大型 C# 代码库要迁移的团队（继续用 C#）。

## 二、不可动摇的约束

这五条是地基。改动它们等于换一门语言，任何提案先对照这里：

1. **始终 GC**——不引入所有权、借用、生命周期。永远不。
2. **字节码就是 IR**——先编到 `.zbc`，永不直接生成机器码。interp / JIT / AOT 都从字节码出发。
3. **默认类型安全**——不安全只存在于边界（`extern` / native 互操作），不做通用逃生舱。
4. **单继承**——一个类只能有一个基类，接口 / `impl` 块可以有任意多个，不留菱形继承。
5. **副作用走显式入口**——I/O 与一切 native 效应只经显式调用或 `[Native]` 标注发生，
   没有隐式的环境魔法。（这条管的是**效应入口**，不是可变性——`static` 字段仍是进程级可变状态。）

> 约束 1 的用户可见形态见[所有权与内存模型](../../reference/src/language/memory-model.md)，
> 实现见 [GC 子系统](runtime/gc.md)；约束 2 见[执行模型](runtime/execution-model.md)；
> 约束 4 见[接口](../../reference/src/language/interfaces.md)。

## 三、语法：C# 的形、Rust 的纪律、Python 的易上手

| 维度 | z42 的选择 |
|---|---|
| **语法底盘** | C#（命名、声明、OOP 结构）|
| **类型拼写** | C# 关键字与 Rust 短名两套等价拼写（`int` ≡ `i32`、`double` ≡ `f64`），**短名是规范形式** |
| **可变性** | 局部变量一律可变；不可变性用 `readonly` 字段和 `const` 表达，不在局部变量上做文章 |
| **错误处理** | 异常。`Result<T, E>` 不在语言里 |
| **抽象** | `interface` + vtable 派发；跨包扩展用 Rust 式 `impl Trait for Type` 块，静态解析 |
| **模块** | 命名空间最多两三段（`Std.IO`、`Std.Net.Sockets`），文件级 `using`，无深层次结构 |
| **所有权** | 无。永远 GC |

**从 C# 砍掉的**：

- **LINQ**——stdlib 没有 `Select` / `Where` 链；集合上只有 `List<T>` 的 `Find` / `Exists` 这类
  eager 谓词族。迭代器链条留作后续设计。
- **指针与 unsafe 块**——不安全只经 `extern` + `[Native]` 这一个口子。
- **协变 / 逆变的完整规则**——泛型只做约束检查，不做变型推理。
- **`Nullable<T>` 与空安全流分析**——见[已知边界](#已知边界)。

> **没砍的**：`global using`（包级 prelude）与 `using Id = T;`（文件级类型别名）都在，
> 见[命名空间与导入](../../reference/src/language/namespaces.md)。
> 命名约定见[命名约定](../../reference/src/conventions/naming.md)。

## 四、Bytecode-native：一份字节码，三档执行

```
源码 → Parser → TypeCheck → IR Codegen → .zbc
                                          ↓
                            ┌─────────────┼─────────────┐
                          interp         JIT           AOT
```

**为什么先落字节码**：

- **interp** 直接吃字节码，没有翻译层——启动即跑，内存足迹小，是 REPL / hot reload / 脚本场景的底座。
- **JIT** 运行期把热函数翻成机器码，对用户代码不可见（无需改一行源码）。
- **AOT** 提前编译，给 iOS / wasm 这类**运行期不允许 JIT** 的平台兜底。⚠️ 后端尚是 stub。

字节码格式必须**对解释器友好**：线性指令流、不做寄存器分配、方法调用 / 类型检查 / 异常处理这些
高层操作与指令一一对应。这条约束优先于「让 JIT 好翻译」——JIT 可以自己再降一层，解释器不能。

> 三档的分工与切换见[执行模型](runtime/execution-model.md)，JIT 后端见 [JIT 后端（Cranelift）](runtime/jit-design.md)，
> AOT 现状见 [AOT 后端](runtime/aot.md)，格式见 [zbc 字节码格式](formats/zbc.md)。

## 五、嵌入优先与 native 边界

z42 VM 的目标形态是**被别的程序嵌进去**，独立可执行文件只是其中一种用法。

四条边界规则：

| 方向 | 规则 |
|---|---|
| **z42 调 native** | `extern` 方法配 `[Native("__name")]`，按名字解析到 VM 的 builtin 表或 ext 表 |
| **宿主调 z42** | VM 导出 C ABI（`z42_host_*`），宿主用 C 或 Rust 都能驱动 |
| **数据共享** | struct 字段布局可预测；GC 指针不跨边界 |
| **调用成本** | 从 z42 调一个 native 函数不应比一次间接跳转更贵——不插 trampoline、不做 marshaling |

> 契约（宿主开发者视角）见[嵌入宿主](../../reference/src/embedding/c-abi.md)与
> [native 互操作](../../reference/src/embedding/native-interop.md)；
> VM 侧实现见 [native ABI](runtime/native-abi.md) 与[嵌入宿主](runtime/embedding.md)。

## 六、动态执行不是可选项

z42 不是纯静态语言。**运行期编译并执行一段新代码**是一等能力，而不是事后补的插件：

- **eval / REPL**——`Std.Scripting` 把「编一轮会话 → 装进活 VM → 求值 / 内省」做成了运行期骨架，
  `z42 repl` 与 `z42 repl -c <expr>` 走的就是它。
- **hot reload**——不重启 VM 就换掉函数实现，为游戏脚本、服务端热更、交互式工具服务。
  ⚠️ 这一条目前**只有设计**，`src/` 里没有实现物。

这条与约束 2（字节码就是 IR）互为因果：因为永远有字节码这一层，运行期换实现才有抓手；
也正因为要支持换实现，字节码必须自带足够的元数据（符号、签名、行号、局部变量名）。

代价要摆明：AOT 产物没有运行期编译器，eval 与 hot reload 在那一档天然不可用。
需要两者兼得的部署形态，走「核心 AOT + 嵌一个 interp 跑脚本」的混合模式。

> REPL 见 [REPL](toolchain/repl.md)，hot reload 的设计见 [hot-reload](runtime/hot-reload.md)，
> 脚本化的长期形态见[脚本化 charter](compiler/scripting-charter.md)。

## 七、始终 GC

**为什么是 GC 而不是所有权**：

- **开发速度**——写逻辑，不写内存管理，没有借用检查器的摩擦。
- **默认安全**——z42 代码里不存在 use-after-free / double-free。
- **可预期**——GC 是已知量：停顿有上界可测、可调参、可诊断。
- **熟悉**——C# / Java / Python 背景的人零学习成本。

这不是妥协。对游戏引擎、服务端、嵌入式框架这类「开发速度比最后一纳秒更值钱」的系统，GC 是**正解**。

代价同样摆明：停顿与堆占用是 z42 要持续投入的方向，不是一次做完的事。

> 实现见 [GC 子系统](runtime/gc.md)；用户可感知的旋钮见[运行时设置](../../reference/src/toolchain/runtime-settings.md)。

## 八、并发：真线程 + 库级原语

z42 的并发设施是**真 OS 线程**：`Thread` 起停、`Channel<T>` 传值、`Mutex<T>` / `RwLock<T>` 护共享状态。
没有 `lock` 关键字——临界区由 `Mutex<T>.With(body)` 这样的库 API 表达，不占语法预算。

线程之间**共享 GC 堆与静态字段**。这意味着：

> **数据竞争由程序员负责，类型系统不拦。** 这是当前的真实状态，不是暂时的实现缺口——
> 「类型层防竞争」需要 `Send` / `Sync` 这一整套设计，属于未来工作。

GC 与线程的协调（safepoint、park）由 VM 承担，对用户代码不可见。

> 用户面见 [z42.threading](../../reference/src/stdlib/threading.md)，实现见[同步原语](runtime/sync-primitives.md)
> 与 [GC 子系统与 safepoint 协议](runtime/gc.md)；async 的长期设计见[并发与 async](runtime/concurrency.md)。

## 九、全栈从简

> **核心准则**：语法、编译器、VM 的实现都**尽量简单**。不为偶发需求增加永久复杂度；
> 每项新特性先回答「能否用已有机制做到」。

三层各自的简化方向：

| 层 | 原则 | 反例 |
|---|---|---|
| **语法** | 少关键字、少歧义点；新语法必须有现有机制做不到的刚需 | 为一点 ergonomic 糖加新 token |
| **编译器** | 优先简单变换（desugar、单次遍历），避免复杂 dataflow | 为一个小特性写专门的 IR pass |
| **VM** | IR 指令集小、builtin 少；primitive 路径走已有指令 | 每个 stdlib 方法都加一个 Rust 特化 |

最重要的推论是 **Script-First**：stdlib 逻辑默认写在 `.z42` 脚本里，只在测到瓶颈时才依次考虑
codegen 特化、VM builtin。判据一句话：**runtime 提供 primitive，feature 一律脚本实现。**
「这样写更快」和「Rust 内部实现更好」都不是下沉的理由。

收益是复合的：stdlib 代码量小、VM 表面积收敛、编译器易自举、逻辑留在可读可替换的脚本层、
interp / JIT / AOT 跑的是同一份脚本。

> 三层落点判据、native 预算、新增 builtin 的改动清单见[实现分层与 native 预算](stdlib/architecture.md)；
> 哪个包允许持有 native 见[包划分与依赖层级](stdlib/organization.md)。
> **改 stdlib 起手先看** `src/libraries/README.md` 的「Extern 现状审计表」——
> 第一选择是消减一个 extern，不是新增。

## 十、性能定位

z42 不是「C 那种快」。它是**一门优化良好的托管语言，在同类里有竞争力**。

分方向的目标：

| 方向 | 目标 |
|---|---|
| **数值代码** | JIT 后与 C# / Java 同级 |
| **面向对象代码** | vtable 派发，与 C++ 虚调用同级 |
| **native 调用** | 零开销——一次间接跳转 |
| **服务端负载** | 分代 GC 给出可预期的延迟 |
| **游戏引擎** | 混合执行：热循环走 JIT / AOT，脚本走 interp |

**明确不追求**：在裸速度上赢 C / Rust。追求的是**不用 unsafe 也够快到能上生产**。

性能目标的具体数值属于计划，不属于哲学——见 `docs/roadmap.md`；
实测口径与回归门禁见[性能基准与回归门禁](devinfra/benchmarking.md)。

> ⚠️ 谈性能前先读 benchmarking 页的基线纪律：**同-runner A/B 是门禁的地基**，
> 拿历史数字跟今天的跑分比会得出假结论。

## 十一、可观测、可调试

字节码自带调试信息，而不是靠外挂符号文件：

- **行号映射与局部变量名**——`.zbc` 的 `DBUG` 段（有行号或局部变量名时才写）。
- **栈回溯**——异常自带可读的调用栈。**仅 interp 路径填充**，JIT 路径目前留空。
- **运行期计数器与采样 profiler**——safepoint 采样出 z42 函数火焰图，计数器可从脚本侧读。

单步调试器尚不存在，取决于是否出现真实需求。

> 见[诊断与性能分析](runtime/diagnostics.md)、[zbc 字节码格式](formats/zbc.md)。

## 十二、语言级可裁剪

一门语言的适用范围由它的**最小可用子集**决定。z42 的长期方向是三条正交的裁剪轴：

1. **VM 组件化**——执行后端、GC、调试按需链接或 dlopen，嵌入式场景只带必需件。
2. **stdlib 树摇**——按 `using` 依赖图只打包实际引用到的包与函数。
3. **语言特性开关**——按工程粒度禁用某些构造（禁 `extern`、要求 switch 穷尽等），
   让同一门语言有「安全子集」与「完整集」两种面孔。

三条都还没实施，各有独立的设计页；这里只登记方向。

> [组件化运行时](runtime/componentized-runtime.md)、[语法定制：三层配置机制](compiler/syntax-customization.md)。

## 十三、演进的态度

z42 的演进分三段，每段有自己的**收特性标准**（当前进度在 `docs/roadmap.md`，不在这里）：

| 阶段 | 关注 | 对新特性的态度 |
|---|---|---|
| **L1** | 最小可用语言：能 parse、能类型检查、能执行 | 只收「不加就到不了 L1 完整」的 |
| **L2** | 生态与质量：打包、stdlib、测试、VM 优化 | 特性推后，先把 L1 钉死 |
| **L3** | 高级特性：ADT、async、函数式模式 | 建在 L1/L2 地基上；架构不干净就不收 |

阶段是**串行的**，但不是铁幕——被自举（dogfood）单点阻断的特性可以按需提前，
泛型、lambda、`impl` 块都是这样进来的。提前的判据是「今天的 z42 子集**真的表达不出来**」，
不是「不够优雅」；**禁止在编译器代码里写绕过**。

**永远不收**：没有清晰用例的复杂度、有歧义的文法、会锁死未来演进方向的特性。

## 已知边界

下面几项是「看起来有、其实没有」，读源码或翻关键字表容易误判：

| 看起来 | 实际 |
|---|---|
| `T?` 是可空类型 | **纯注解，类型解析时被擦除**。没有 `Nullable<T>`，没有空安全流分析，`string s = null;` 编得过 |
| `async` / `await` 是关键字 | **词法器认得，语义层没有任何消费者**。没有 `Task` / `ValueTask`，没有线程池 |
| 泛型是单态化的 | **代码共享 + 运行期具化**——一份字节码服务所有实例化，type_args 在 TypeDesc 里。源码注释说的「类型擦除」只指方法体内看不到具体 class name |
| 有 hot reload | 设计页在，`src/` 里**零命中**。`[HotReload]` 不是编译器认识的 attribute |
| `[ExecMode]` 是 attribute | 编译器的内建 attribute 集里**没有它**。ExecMode 是 zbc 的每函数元数据字节加 CLI 旗标，IrGen 目前一律发 `Interp` |
| 有 `match` 表达式 | 结构化模式长在 `switch` 上；穷尽性目前是**警告**（W0700），且只覆盖 `bool` / `enum` / 封闭的非公开类层次 |
| 有 `trait` | 关键字是 `impl`；被 `impl` 的那个东西是 `interface` |
| 有 AOT | `aot.rs` 是自述的 stub。规划的后端是 cranelift-object，**不是 LLVM** |
| 有 `[language]` 特性开关 | `LanguageFeatures` 类存在且有 21 个开关位，**零调用方**；工程清单也不解析这一节 |

另有几项是明确的**未来工作**，不是缺陷：`Result<T, E>` 与 `?` 传播、原生 ADT 与穷尽 `match`、
迭代器链、`Send` / `Sync` 类型层并发安全、局部变量不可变性（`let` / `mut`）。
它们的排期在 `docs/roadmap.md`。
