# z42 闭包设计

> **Status**: L3-C2 ✅ core+loops+jit ｜ lambda 字面量 + 捕获 + 档 C 堆擦除 + JIT；档 A/B 完整版 + Send 派生见 Deferred

> 本文档是 z42 闭包 / lambda / 函数类型的**权威设计规范**。
> 上游：[`philosophy.md`](../philosophy.md)（设计原则）/ [`language-overview.md`](language-overview.md)（用户视角语法）
> 下游：[`grammar.peg`](grammar.peg)（机器可读文法）/ [`ir.md`](../../internals/src/formats/ir.md)（IR 指令）/ [`concurrency.md`](../../internals/src/runtime/concurrency.md)（spawn 捕获规则）
> 相关：[`iteration.md`](iteration.md)（高阶 API 用例）/ [`customization.md`](customization.md)（L3 lambda 定位）

---

## 1. 概述

z42 的闭包模型可以一句话概括为：**C# delegate 减去 MulticastDelegate**——单一统一类型、隐式捕获、无 `+=` 多播。

设计在易用性（C# 风格）的基础上修正了 C# 的几个经典陷阱，并通过编译器的三档实现策略让性能曲线达到 Rust 级别。

```z42
// 用户视角是一种代码：
var inc = (int x) => x + 1;              // lambda 字面量（var 推断为 (int)->int）
int Square(int x) => x * x;              // 表达式短写（C# 风格函数声明）
list.Map(x => x * 2);                    // 高阶 API 用法
list.Filter(x => x > 0);                 // 高阶 API 用法
```

---

## 2. 设计哲学

| 原则 | 落点 |
|------|------|
| **易用优先（C# 风格）** | 单一类型、隐式捕获、`=>` lambda、不引入 capture list |
| **去 C# 经典陷阱** | 值类型快照（消除"循环变量晚绑定"）、单目标（消除多播 4 类陷阱） |
| **性能可达 Rust 级别** | 编译器三档自动选：栈分配 / 单态化 / 堆擦除——用户语法不变 |
| **大项目友好** | 类型擦除路径（档 C）默认可用，避免 Rust 单态化的代码膨胀 |
| **与并发模型对齐** | spawn 边界自动 move + `Send` 派生（详见 [`concurrency.md`](../../internals/src/runtime/concurrency.md)） |
| **泄漏靠诊断，不靠语法** | 不引入 `[weak self]` / `\|capture this\|` 类标注；定位泄漏由 GC 诊断负责 |

---

## 3. 语法

### 3.1 Lambda 字面量

```z42
var f1 = x => x + 1;                     // 单参表达式
var f2 = (x, y) => x * y;                // 多参表达式
var f3 = () => 42;                       // 无参
var f4 = x => {                          // 语句体
    var y = x * 2;
    return y + 1;
};
var f5 = (x: int, y: int) => x + y;      // 显式参数类型
```

- 表达式 body 隐式返回；block body 必须 `return`
- 单参可省括号；多参 / 显式类型必须 `(...)`
- Lambda 字面量必须出现在表达式位置

### 3.2 函数类型 `(T) -> R`

```z42
(int) -> int                             // 单参
(int, string) -> bool                    // 多参
() -> int                                // 无参
(int) -> void                            // 无返回值（C# 一致使用 void）
((int) -> int) -> int                    // 高阶
List<(int) -> bool>                      // 作为泛型实参
```

**类型位置写法对比**（2026-05-02 add-delegate-type 后）：

| 写法 | 用法 | 推荐度 |
|---|---|---|
| `(int) -> int` 字面量箭头类型 | 简短匿名场景；签名一目了然 | ⭐⭐⭐ 默认 |
| 命名 `delegate int IntFn(int x);` | 跨多 site 复用 / 接 C# 心智 / 嵌套类型组织 | ⭐⭐⭐ 推荐 |
| 泛型 `Func<int, int>` / `Action<int>` / `Predicate<int>` | C# 风格，由 stdlib `Std.Delegates` 提供（D1c） | ⭐⭐ 兼容 |

这三种写法**结构等价**，互转零开销。脚本生成 N arity 类型避开 C# 17 重载维护成本。详见 `delegates-events.md` §3。

### 3.3 函数表达式短写（C# 7+ expression-bodied）

```z42
int Square(int x) => x * x;
string FullName(Person self) => self.first + self.last;
```

等价于 `{ return ...; }` 形式；必须以 `;` 结尾；`=>` 后必须是表达式而非 block。这就是 C# 的 expression-bodied 成员，z42 在顶层函数和方法位置都启用。

### 3.4 局部函数（嵌套函数声明，C# 7+）

```z42
int Outer() {
    int Helper(int x) => x * 2;
    int Fact(int n) => n <= 1 ? 1 : n * Fact(n - 1);   // 直接递归
    return Helper(3) + Fact(5);
}
```

- 词法作用域：仅在所属 block 内可见
- 直接递归合法（名字在自身 body 内可见）
- L2 阶段：嵌套函数不允许引用外层 local（捕获是 L3 特性）
- L3 阶段：捕获非空时升级为闭包，按 §4 规则处理

---

## 4. 捕获语义

### 4.1 值类型按快照捕获

```z42
var x = 5;
var f = () => Console.WriteLine(x);
x = 10;
f();   // 输出 5（不是 10）
```

- 创建闭包时**拷贝**值类型，后续外部修改不影响闭包
- 与 C# 不同（C# 把值类型 hoist 到 display class，按引用共享）
- 这条规则**消除了"循环变量晚绑定"**和**值类型幻读**两类经典陷阱

### 4.2 引用类型按对象身份共享

```z42
class Counter { public int n = 0; }
var c = new Counter();
var inc = () => c.n = c.n + 1;
inc(); inc();
Console.WriteLine(c.n);   // 输出 2
```

- 捕获的是**对象身份**，闭包内外见同一对象
- 外部重新指向变量槽**不影响**已创建闭包：

  ```z42
  var c = new Counter();
  var inc = () => c.n = c.n + 1;
  c = new Counter();   // c 指向新对象
  inc();               // 仍操作原对象
  ```

### 4.3 循环变量每次迭代新绑定

所有循环（`for` / `foreach` / `while`）每次迭代为循环变量创建新绑定：

```z42
var fns = new List<() -> int>();
foreach (var i in new[] { 1, 2, 3 }) {
    fns.Add(() => i);
}
foreach (var f in fns) Console.WriteLine(f());   // 输出 1, 2, 3（不是 3, 3, 3）
```

C 风格 `for` 同样适用——这是与 C# 5+ 的一处对齐扩展（C# 5 只修了 foreach）。

> **实现注**：z42 的"每次迭代新绑定"语义由值快照规则（§4.1）**自动满足**——
> 闭包 env 在 `MkClos` 时拷贝当前迭代的值，而非持有"对循环变量的引用"。
> 即使 `for` / `foreach` 循环用单一寄存器复用循环变量存储，每次创建的闭包
> env 仍是独立快照。无需循环 codegen 特殊处理。
> 见 `golden/run/closure_l3_loops/`（regression 防护）+
> `docs/spec/archive/2026-05-02-verify-closure-l3-loops/`（验证记录）。

### 4.4 共享可变状态：用 class（不引入 `Ref<T>` 包装）

由于值类型按快照捕获，需要"在闭包内修改外部状态"时——**直接用 class**（引用类型按身份共享规则自然生效）：

```z42
class Counter { public int n = 0; }

var c = new Counter();
var inc = () => c.n = c.n + 1;
inc(); inc();
Console.WriteLine(c.n);   // 输出 2
```

z42 **不引入** Rust 风格的 `Ref<T>` / `Box<T>` 堆包装类——刻意保持简单。
共享可变状态的唯一推荐路径是 class（引用类型）。

> **C# `ref` 关键字**是函数参数级特性（pass-by-reference 调用），与闭包**不相关**——它不可被 lambda 捕获（生命周期不允许，与 C# 一致）。本规范不规定 `ref`，那是独立 feature 提案 `add-ref-params` 的范畴。

### 4.5 编译期诊断：W0604 "写入值类型 captured var"

§4.1 的"值快照捕获"是 design feature，但调用方期望 C# 风格的引用共享时
是 silent footgun ——尤其是 `Thread.Start(() => { result = ...; })` 这类
跨线程发布结果的模式：写的是 closure 局部副本，主线程读不到。

z42 编译器在 TypeCheck 阶段对"闭包内写值类型 captured var"emit
`W0604` warning（add-warn-captured-value-snapshot-assign, 2026-05-30）：

```text
warning W0604: assignment to captured value-type variable `seen` is
local to the closure; outer scope keeps its original value. To share
mutable state across the closure boundary, wrap in a class (see
docs/design/language/closure.md §4.4) or use a single-element array
as a cell.
```

修复有两条路：

```z42
// 路 A — class wrap（§4.4 推荐）
class Cell { public bool seen = false; }
var cell = new Cell();
Thread t = Thread.Start(() => { cell.seen = true; });
t.Join();
// cell.seen 是 true（共享对象身份）

// 路 B — 1-element array cell（轻量惯用法，stdlib 测试常用）
bool[] seen = new bool[1];
Thread t = Thread.Start(() => { seen[0] = true; });
t.Join();
// seen[0] 是 true（array 是引用类型）
```

写**引用类型** captured var 的 field（`cell.seen = ...`）或 array element
（`seen[0] = ...`）**不触发** W0604 —— 那是在写已捕获对象的内部状态，
正是 §4.2 推荐的共享模式。仅"赋值到 captured var 自身"才报警。

W0604 是 warning 而非 error：§4.1 把"局部副本"行为定为合法语义，可能
有罕见用例确实想要 closure-local effect；warning 提示常见误用又不阻塞
编译。`z42c explain W0604` 给出详细文档。

---

## 5. 单目标闭包（无多播）

z42 闭包是**单目标值**——不支持 C# 的 `+=` / `-=` 多播组合。

```z42
(int) -> void f = x => Console.WriteLine(x);
f += x => Console.WriteLine(x * 2);   // ❌ 编译错误：函数类型不支持 +=
```

**为什么砍掉多播**：C# 多播带 4 个典型陷阱——
1. 中途异常吞掉后续 handler
2. 返回值仅取最后一个，前面静默丢弃
3. `-=` 退订对内联 lambda 失效（每个字面量是新实例）
4. 长寿事件源持有闭包导致泄漏

事件 / 多订阅场景由独立的 `EventEmitter<T>` 库类型承担，安全性更显式：

```z42
var bus = new EventEmitter<int>();
var sub = bus.Subscribe(x => Console.WriteLine(x));   // 返回 Subscription handle
bus.Emit(5);
sub.Dispose();                                        // 通过 handle 退订，与闭包身份无关
```

---

## 6. 实现策略：编译器自动选三档

用户**永远写一种代码**；编译器根据闭包出现的位置自动选三档之一。

### 6.0.1 档 A 子集（已落地：单变量立即调用，2026-05-02 impl-closure-l3-escape-stack）

档 A 完整版（"任何不逃逸的 closure 都栈分配"）尚未落地，但已实现一个**子集** —— **"capturing closure 仅作 callee 立即调用"** 模式：

```z42
int n = 5;
var add = (int x) => x + n;       // ← capturing closure
var r = add(3);                    // ← 仅作 callee 调用，无其它 use
```

满足条件的 closure：编译器把 `MkClos.StackAlloc=true` → VM 在创建 frame 的 `env_arena: Vec<Vec<Value>>` 中放 env，闭包值是 `Value::StackClosure { env_idx, fn_name }`。**零堆分配 + 零 GC root 占用**。

不满足任一条件即 fallback 堆（`Value::Closure`，原 Tier C 路径），保留正确性：

| 失败模式 | 原因 |
|---|---|
| `return closure;` | 跨 frame escape |
| `field = closure;` / `arr[i] = closure;` | 引用泄露到长寿对象 |
| `Run(closure)` 传给 callee | 第一版无 `[NoEscape]` 标注，所有 callee 默认逃逸 |
| `var g = f;` 别名链 | 第一个 var 的"用途"非 callee 位置 |
| `f = OtherClosure;` 重赋值 | env lifetime 不再单一 |
| 嵌套 lambda 捕获 closure var | `BoundCapturedIdent` → escape |

**实现概览**（详见 `docs/spec/archive/2026-05-02-impl-closure-l3-escape-stack/`）：

- `Value::StackClosure { env_idx: u32, fn_name: String }` —— 与 `Value::FuncRef` / `Value::Closure` 对称的第三种 callable variant，dispatch 端 match 三个 variant（design Decision 1 选项 A）
- `InterpFrame.env_arena` / `JitFrame.env_arena` —— per-frame `Vec<Vec<Value>>`，stack closure 创建时 push、CallIndirect 时 clone 内容升格为临时 GcRef 给 callee（callee 不区分 stack/heap closure）
- `VmContext.env_arena_stack` —— 与 `exec_stack` 平行的 GC root stack，scanner 扫 arena 中的 Value 完成对象图覆盖；`push_frame_state(regs, env_arena)` API 强制 1:1 push
- `ClosureEscapeAnalyzer` —— TypeChecker 后置 pass：找出 `var x = lambda;` 候选 → 扫函数体确认所有 `BoundIdent(x)` 都在 `BoundCall.Receiver` 位置 → 加入 `SemanticModel.StackAllocClosures` 集合 → Codegen 透传到 `MkClosInstr.StackAlloc`
- 保守优先：分析器**绝不冒**误判 escape 为 stack 的风险（→ use-after-free），任何不确定即 fallback heap

未来增量：`[NoEscape]` 形参标注 / stdlib 高阶 API 白名单 / 流敏感分析 / 完整档 A（任何"明显不逃逸"的 closure）。

### 6.1 档 A — 栈分配

**触发**：闭包传给具体 `(T) -> R` 形参，且形参标注 `[no_escape]`（或流分析确认不逃逸）。

```z42
items.Filter(x => x > 10);
```

env 在调用方栈帧上分配，调用是直接 call。**0 次堆分配**。

### 6.2 档 B — 单态化 + 内联

**触发**：闭包传给泛型 `<F: (T) -> R>` 形参，单态 call site。

```z42
List<U> Map<T, U, F>(this List<T> self, F f) where F: (T) -> U { ... }
list.Map(x => x * 2);   // 编译期为该 lambda 单态化 Map，闭包体内联展开
```

**0 次堆分配 + 0 次函数调用开销**——等同手写 for 循环。代价：每个唯一闭包类型多一份 `Map` 二进制（代码膨胀）。

### 6.3 档 C — 堆擦除

**触发**：闭包逃逸（存字段 / 入集合 / 跨 spawn / 作返回值）。

```z42
class EventBus {
    public List<(int) -> void> Handlers = new();
}
bus.Handlers.Add(x => Console.WriteLine(x));
```

env 进 RC/GC 堆，闭包对象是胖指针 `(env_ptr, vtable_ptr)`，调用走 vtable 间接调用。性能与 C# delegate 等同。

### 6.4 三档对比

| 维度 | 档 A 栈 | 档 B 单态化 | 档 C 堆擦除 |
|---|---|---|---|
| env 位置 | 调用方栈帧 | 不存在（inline） | RC/GC 堆 |
| 堆分配 | 0 | 0 | 1（每次创建）|
| 调用方式 | 直接 call | 内联展开 | vtable 间接 |
| 代码膨胀 | 无 | 有（每闭包类型一份）| 无 |
| 跨线程能力 | ❌ | ❌ | ✅（env 在堆 + Send） |
| 典型用途 | map/filter 链式 | stdlib 极热路径 | 事件 / 异步 / 集合存储 |

### 6.4.1 单态化（已落地：alias 子集，2026-05-02 impl-closure-l3-monomorphize）

档 B 的全套（"泛型形参 + 单态 call site + 内联"）尚未落地，但 z42 已实现一个**子集** —— **alias 单态化**，把"已知 callee 走 CallIndirect"的低效 case 折叠为直接 `Call`：

| 模式 | 改前（一律堆擦除路径）| 改后（alias 单态化）|
|---|---|---|
| `Helper();` | `Call("ns.Helper", ...)` | 不变 |
| `var f = Helper; f();` | `LoadFn` + `CallIndirect` | `Call("ns.Helper", ...)` ✓ |
| `var f = Helper; var g = f; g();` | `LoadFn` ×2 + `CallIndirect` | `Call("ns.Helper", ...)` ✓（alias 链传播） |
| `f = OtherFn; f();` | CallIndirect | CallIndirect（赋值后 alias 失效） |
| `Apply(Square, 7)` 内 `f(x)` | CallIndirect（不变） | CallIndirect（参数 callee 编译期不可知） |

**实现原理**（详见 `docs/spec/archive/2026-05-02-impl-closure-l3-monomorphize/`）：

- TypeChecker 在 `var f = Helper;` 这种"init 是 simple ident 且 ident 解析为已知函数"的情形把 `f` 写入 `TypeEnv._funcAliases[f] = "ns.Helper"`
- 任何对 `f` 的赋值清掉 alias（保守）
- 嵌套作用域随 `TypeEnv._parent` 链查 alias —— `var g = f;` 复用 `f` 的 alias
- BindCall 在"var of FuncType"分支前先查 alias：命中 → emit `BoundCall(Free, null, …, CalleeName=fqName)`，与"直接调用顶层函数"路径同构 → Codegen 直接 `Call`
- `Apply(Helper, …)` 中 `Helper` 通过 ident 装载为 FuncRef（Codegen 新增 `BoundIdent → LoadFn` 路径，覆盖 ident 解析为顶层函数 / 静态方法的情形），保持值传递语义
- 仅做单赋值跟踪（design Decision 2）；条件分支 / 字段读取的复杂情形保守 fallback CallIndirect
- **不影响带捕获的 closure**（`var add = (int x) => x + n;` 仍走 MkClos + CallIndirect，因为单态化丢 env 会破坏语义，design Decision 4）

未来增量：流敏感分析（if/else 二选一）/ 跨 capture 边界 alias 传播 / 与档 B 单态化合并。

### 6.5 决策算法

```
对每个闭包字面量 L:
  1. context 是泛型形参 <F: (T) -> R>？        → 档 B
  2. context 是具体 (T) -> R 形参 + 不逃逸？    → 档 A
     ╰ 形参或函数标了 [no_escape]，或流分析证明
  3. context 是字段赋值 / 集合插入 / spawn / 返回值？ → 档 C
  4. context 是 var / let 绑定？               → 流分析后递归归类
  5. 兜底（无法判定）                          → 档 C（保守）
```

**关键依赖**：stdlib 高阶 API 形参必须显式标 `[no_escape]`，否则编译器只能保守归到档 C。这是结构性属性，不是用户级断言。

---

## 7. 性能诊断

提供一个编译选项让档 C 的发生位置可观测：

```bash
z42 build --warn-closure-alloc
# 输出每个走档 C 的闭包字面量位置 + 原因
```

> 不引入 `[expect_no_alloc]` 之类的用户级硬断言属性。档 C 触发条件（字段 / spawn / 返回 / 集合）天然不在热路径，warning 通道已足够；引入硬断言只是把警告升级为错误，价值与新增语法面不匹配。

---

## 8. 类型行为

### 8.1 可比较

闭包支持 `==` / `!=`，按 (target, function) 对相等：

```z42
var f = (int x) => x + 1;
Console.WriteLine(f == f);   // true
var g = (int x) => x + 1;
Console.WriteLine(f == g);   // false（不同字面量 → 不同对象）

var refA = MyFn;
var refB = MyFn;
Console.WriteLine(refA == refB);   // true（target=null + 同 function pointer）
```

### 8.2 不可序列化

```z42
serialize(closure);     // ❌ 编译错误：函数类型不可序列化
```

闭包不实现序列化接口。C# 历史上 `BinaryFormatter` 支持 delegate 序列化，但已正式弃用（版本脆弱 + 安全漏洞），z42 直接不引入。

---

## 9. 并发对接

`spawn` / `task` / async 等"逃逸到独立执行上下文"语境，闭包**强制 move 捕获**，且捕获项必须 `Send`：

```z42
task scope {
    var data = LoadData();
    spawn { Process(data); };   // data 移动到闭包；spawn 后再用 data → 编译错误
}
```

详细规则见 [`concurrency.md`](../../internals/src/runtime/concurrency.md) §6.3——本节给出契约，那里给出实现细节。

`Send` 由编译器**自动派生**：闭包类型 Send ⇔ 所有捕获项类型 Send。无需用户手动声明。

---

## 10. L 阶段定位

| L 阶段 | 允许的闭包特性 |
|---|---|
| **L2** | 无捕获 lambda + `(T) -> R` 函数类型 + 表达式短写 + 无捕获 local function |
| **L3** | 完整闭包（捕获 / 三档实现 / Send 派生 / `--warn-closure-alloc` 诊断）|

L2 阶段提供"语法骨架"，覆盖 90% 的回调字面量（`callback(x => x.Name)` 这类不捕获外部状态的写法）；L3 阶段补齐捕获能力，配合内存模型决议后落地。

L2 阶段编译器对捕获非空 lambda **直接编译错误**——不静默降级，避免给后续 L3 实现留下兼容包袱。

---

## 11. 与现有规范的交叉引用

| 文档 | 关系 |
|------|------|
| [`philosophy.md`](../philosophy.md) | 闭包设计是"C# 易用 + Rust 性能"原则的具体落点 |
| [`language-overview.md`](language-overview.md) | 用户视角语法概览，引用本规范的 §3 / §4 / §10 |
| [`grammar.peg`](grammar.peg) | LambdaExpr / FnTypeExpr / 表达式短写 / 嵌套函数文法 |
| [`ir.md`](../../internals/src/formats/ir.md) | `mkclos` / `callclos` / `mkref` / `loadref` / `storeref` 指令 |
| [`concurrency.md`](../../internals/src/runtime/concurrency.md) | spawn move + Send 规则；本规范 §9 给契约，那里给实现细节 |
| [`iteration.md`](iteration.md) | Map/Filter/Reduce 等高阶 API 是闭包的主要消费方 |
| [`customization.md`](customization.md) | L3 lambda 定位由本规范 §10 取代 |

---

## 12. 待内存模型决议后回填

以下细节**依赖 z42 内存模型决议**（RC vs GC vs 混合），暂作占位：

- 档 C 闭包的 env 在 RC 模型下的具体编码（refcount 字段位置 / drop 时机）
- 档 C 闭包的 env 在 GC 模型下的扫描根注册
- 弱引用支持（如未来真实现，作为独立 IR 指令，不混入闭包 spec）
- VM 诊断需求：对象引用链反向追溯、captured env 内容 dump、allocation site 追踪——属 `vm-architecture.md` 的工作范畴

---

## Open Questions

以下问题不影响本规范的核心契约，留待具体实现阶段验证：

- **函数表达式短写歧义**：`R Name(T x) => expr;` 与下一条声明在 PEG 文法中是否需要额外消歧规则——`grammar.peg` 同步任务（task 2.2）实证。z42 现有代码已大量使用此形式（参见 `examples/generics.z42` 的 `Map`/`UnwrapOr` 等），文法上应已消歧。
- **"无捕获 lambda"边界**：常量字面量（`x => 42`）算不算捕获？规定**不算**——编译期内联处理，不进入捕获分析。
- **闭包类型 vs 函数指针类型**：z42 **不引入** C / Rust 风的独立函数指针类型；无捕获闭包在 IR 层降级为函数引用，但用户视角统一是 `(T) -> R`。
