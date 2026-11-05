# 标准库的实现分层与 native 预算

> 代码：`src/libraries/`（z42 源）、`src/runtime/src/corelib/`（builtin 实现）、
> `src/compiler/z42c.emission/src/Emission/StubEmitter.z42`（`[Native]` 分流）
>
> 包怎么划分、新包怎么开 → [包划分与依赖层级](organization.md)。
> builtin 表结构 / `BuiltinId` 不变式 / typed native 通路 → [native ABI](../runtime/native-abi.md)。
> 用户怎么调这些 API → 参考手册 [标准库](https://z42-lang.github.io/z42/reference/stdlib/index.html)。

本页写**一个 stdlib 方法该落在哪一层实现**、全仓 native 表面今天有多大、以及新增一个 builtin 要改哪几处。

## 1. 实际只有两层半

```
┌──────────────────────────────────────────────────────────┐
│  脚本层   src/libraries/*.z42                            │
│  绝大多数 stdlib 逻辑；编译成 .zpkg，与用户代码同等对待   │
├──────────────────────────────────────────────────────────┤
│  codegen 特化   z42c.emission/{Operator,Expr}Emitter     │
│  primitive 算子直降 IR 指令，既不过 VM dispatch 也不占    │
│  builtin 名额（`+` → Add，`<` → cmp + jmp）              │
├──────────────────────────────────────────────────────────┤
│  VM builtin   src/runtime/src/corelib/                   │
│  Rust 函数；`[Native("__x")]` 按名字在调用期解析          │
└──────────────────────────────────────────────────────────┘
```

**没有独立的「Platform HAL」层。** 仓里不存在 `Platform` trait，也没有 `NativePlatform` /
`WasmPlatform` 实现——builtin 直接调 `std::`。平台差异由两个更窄的机制承担，各自只覆盖自己的
concern：

| 机制 | 覆盖 | 位置 |
|---|---|---|
| **PAL** | OS 差异（`#[cfg(target_os)]` 的唯一归集地），按 concern 分阶段迁入 | `src/runtime/src/pal/`，见 [PAL](../runtime/pal.md) |
| **VFS** | 每台 VM 一个文件系统句柄：本地文件系统或内存 VFS（wasm），由创建 VM 的一方选 | `pal/fs.rs`（`Vfs` trait）+ `pal/vfs/{local,memory}.rs` |

其余 builtin（`__net_*` / `__thread_*` / `__math_*` …）没有抽象层，直接落在 `corelib/<area>.rs`
里用 `std::` 或第三方 crate 实现。要加平台时是逐 concern 往 PAL 搬，不是补一个统一 HAL。

## 2. 落点决策：Script-First

新增一个 stdlib 方法时按顺序试：

1. **脚本实现**（`.z42`）—— 默认答案，即使当下不够快。
2. **codegen 特化** —— 调用能直接映射到既有 IR 指令（算术、比较、短路、字符串拼接）时，在
   IrGen 识别并替换，**不新增 builtin**。
3. **VM builtin** —— 只在满足以下之一时：
   - 必须访问 OS / native 资源（syscall、libm、时钟、熵、线程、socket）；
   - VM 元数据依赖（类型对象、反射、GC 内省、装箱布局）；
   - 有实测依据的热点，且第 2 条无法覆盖。

判据可以压成一句：**runtime 提供 primitive，feature 一律脚本实现。**

| | primitive（可以是 builtin） | feature（必须脚本） |
|---|---|---|
| 定义 | JIT 消不掉的硬能力 | 能用已有 primitive 组合出来的逻辑 |
| 例 | syscall、libm FPU、GC barrier、类型元数据、UTF-8 codepoint 访问、数值字面量 parse | 集合算法、格式化、Assert、字符串拆分拼接、Path 字符串操作、abs/max/min |

**「这样写更快」不是理由。** 性能升级阶梯是：脚本实现 → 优化脚本层本身（JIT、算法、调用机制）
→ 仍不达标才下沉。VM 内置是最后手段——投资通用机制的收益比逐个下沉高，且能把逻辑留在脚本层。

**「Rust 内部优化」也不是理由。** `StringBuilder` 用 Rust 可变 `String` 加速属于内部优化，不是
native 库依赖；它已是纯脚本（`z42.text`，基于 `List<string>` + `String.FromChars`）。

字符串内部为 UTF-8。`String.CopyUtf8To` 复制现有存储字节，`String.FromUtf8` 从独立快照严格构造合法字符串；
它们属于存储边界，供纯脚本 encoding 组合使用，不暴露可变视图。可变 byte[] 经已有原子分块区间原语读写，
不能对并发数组直接 memcpy。构造时校验与发布使用同一份快照，避免校验后再次读取可变数组。

落点判断的若干实例：

| 调用 | 落点 | 理由 |
|---|---|---|
| `x + y`（int/long/double/string） | codegen 特化 | IR 已有指令，对 primitive 零开销 |
| `x.CompareTo(y)`（primitive） | 脚本 | `if (this < other) …` 走 IR cmp+jmp，与 Rust `partial_cmp` 等价 |
| `Math.Abs / Max / Min` | 脚本 | 三元判断，BCL/Rust 同样不是 intrinsic |
| `Math.Sqrt / Sin / Exp` | builtin | libm 精度 |
| `Path.Join / GetExtension` | 脚本 | 纯字符串操作 |
| `File.ReadAllText` | builtin | syscall |
| `"abc".CharAt(1)` | builtin | UTF-8 codepoint 访问 |
| `new List<T>().Add(x)` | 脚本 | `T[]` 之上可表达 |
| `Array.MayContainReferences(array)` | builtin | 只读数组存储模式；打包基元返回 false，其余布局保守返回 true，不扫描元素或构造反射对象 |

数组引用探针读取不可变的 `ArrayMode`，不分配、不写堆，也不调用脚本。REF/MIXED/STRUCT/FROZEN
均保守返回 true，包括空引用数组和没有引用字段的 struct 数组；不能把“当前元素都是 null”当成无引用布局。
容器可在创建 backing array 后缓存结果，跳过基元废弃槽的无用写入；非基元的清槽和 GC 写屏障仍由既有数组赋值承接。

`StringBuilder` 的片段数组在逻辑范围外不持有废弃引用。Clear 清空已用片段，保留数组容量；
编辑收敛为单段时清除旧尾槽。清槽通过脚本数组赋值完成，既有字符串快照不受影响。

### 文本流的有限缓冲

`StreamReader` 继承 `TextReader` 的单字符核心和生命周期接口，自有批量与按行读取路径。
固定 4096 字节缓冲中的完整 UTF-8 前缀经 `Encoding.GetString` 严格解码；最多三个合法但不完整的尾字节
移到缓冲开头等待后续数据。当前块的不可变字符串用于按块输出，Unicode 标量数组用于 O(1) 游标访问。
Peek/Read/ReadLine 已取得完整字符便返回，不等待缓冲填满或源 EOF；ReadToEnd 在同一固定缓冲内合并短读再解码，
避免每标量转换。源异常时保留本块已经成功取得的字节供后续操作继续；正常解码后最多保留三个尾字节。
跨块 CRLF 的 LF 延迟到下一次操作跳过，避免首行额外等待。

EOF 永久化并解除源和工作缓冲引用，Close/Dispose 同样解除引用而不关闭源；预读内容不回退底层位置。
解码失败进入不可继续读取状态；抛异常的那次源 Read 不提交字节，此前成功返回的字节与尾前缀保持可重试。
内部工作缓冲固定有界，ReadToEnd 和跨块长行的输出空间仍与结果长度成比例。

包装流的持有周期与底层流的打开状态分开管理。`StreamWriter.Close` 成功刷新后标记关闭并解除目标流、
编码引用，调用方仍可继续使用自己持有的目标流。刷新失败时保持 writer 打开并保留目标，允许修复源错误后重试；
重复 Close 或关闭后的 Flush 不再访问目标。

## 3. `[Native]` 的两种形态

`[Native]` 是编译器原生封闭集里的 **directive**（`HandlerRegistry.IsNativeDirective`），不走
store-meta 的反射工厂路径。`StubEmitter._emitNativeStub` 按有没有 `type=` 分流成两条完全不同的
通路：

| 写法 | 发的指令 | 解析方式 |
|---|---|---|
| `[Native("__str_length")]` | `BuiltinInstr` | 名字 → `BUILTINS[]` 下标，加载期解析，未命中留 `UNRESOLVED` 调用期再解 |
| `[Native(lib = "z42_compression", entry = "__zstd_compress")]` | `BuiltinInstr`（**同一条**） | 同上；`lib=` 不参与解析，只是给人看的标注。实际符号由 ext loader 注册进 per-VM 的 ext 表 |
| `[Native(lib =, type =, entry =)]` | `CallNativeInstr` | typed native type 通路，libffi |

第三种在 stdlib 里没有使用者。前两种的表结构、`BuiltinId` 烤进 zbc 的不变式、ext 表高位打标，
全部归 [native ABI](../runtime/native-abi.md)，本页不重复。

方法体必须省略（`;` 结尾），`extern` 关键字与 `[Native]` 同时写：

```z42
[Native("__thread_spawn")]
public static extern long Spawn(object action);
```

实例方法与属性也支持——`Std.Type` 的 `FullName` / `IsEnum` 等都是 `public extern … { get; }`。

**名字写错不会在编译期报错。** 编译器不查 builtin 注册表，一个不存在的 `__foo` 能顺利编过，到
调用时才炸（实测：`Std.MissingSymbolException: undefined function Bad.Nope$1$i32`）。新增 builtin
时这条决定了验证只能靠跑。

## 4. 新增一个 builtin 要改哪里

1. `src/runtime/src/corelib/<area>.rs` —— 在该文件的 `#[z42_macros::builtins]` 模块里写一个类型化函数
   `#[builtin(name = "__…")] fn f(…) -> Result<T, BuiltinError>`（新模块要在 `builtin_table.rs` 的 `TYPED`
   里加一行；签名约定、错误类型、`blocking` 见 [native ABI §6.2](../runtime/native-abi.md)）。
2. `src/libraries/z42.core/src/…` —— 写 `[Native("__…")] public static extern` 声明；归属见
   [包划分](organization.md)。声明只能落在 core，或写了 `lib=` 的运行时非必需独立库；编译器 / 工具链包不行
   （[extern 落点](organization.md#extern-落点只有两处)）。

没有第三处：z42c 不维护 builtin 签名表，签名就是那条 z42 声明本身，参数个数与类型也不在编译期对账
（不一致时调用即报内部错误，靠测试发现）。命名约定（`__<area>_<verb>`）与 area 清单在
[native ABI §6.5](../runtime/native-abi.md)。

## 5. native 表面与对账

VM 注册表由各 `#[builtins]` 模块的 `DESCS` 组合，实际数量随构建 feature 变化。
`corelib/native_decl_tests.rs` 对源码声明、注册符号、豁免项及返回类型做双向检查；新增接口须在同一次变更中满足对账。
在 `src/runtime/` 运行 `cargo test --lib native_decl_tests` 可验证当前构建的实际表面。

| 包 | native 归属 | 性质 |
|---|---|---|
| `z42.core` | 执行基座、平台语义层及字符串/数组/类型元数据存储原语 | 语义汇聚核 |
| `z42.harness` | `lib = "z42_harness"` | 运行时非必需，实现在 VM builtin 表 |
| `z42.diagnostics` | `lib = "z42_diagnostics"` | 运行时非必需，实现在 VM builtin 表 |
| `z42.compression` | `lib = "z42_compression"` 的 `entry` | 运行时非必需，独立 cdylib |

compression 由 cdylib 加载时注册到 per-VM ext 表，见 [native ABI §6.6](../runtime/native-abi.md)。
其余能力库保持零 extern，需要存储或平台能力时经 core 或独立能力库公开接口调用；具体落点见
[包划分规则](organization.md#extern-落点只有两处)。

### 表里没有 stdlib 声明的条目

一部分是编译器直接发射的（`__box_prim` / `__box_struct` / `__methodof` / `__struct_hash_code` /
`__class_type_arg` / `__class_default`），一部分住在宿主侧（`__repl_*`）。清单与理由在
`corelib/native_decl_tests.rs` 的 `UNDECLARED_ALLOWLIST`；表项可以真删——`BuiltinId` 不跨进程持久化，
判据只是「已发布种子里还有没有 z42 源声明这个名字」（零引用的 `__concat` / `__len` / `__contains`
已按此判据连同实现删除）。

> 审计时注意：`grep "extern"` 会大量命中注释——`z42.io` / `z42.net` / `z42.json` / `z42.text` 里
> 提到 extern 的行全是「本文件不自带 extern，已上移 core」这类说明。要数真实声明必须匹配
> `[Native(` 或带修饰符的 `extern` 声明行。

## 6. 加载与可见性

**`z42.core` 是隐式 prelude**（`DepScan.IsPrelude`，硬编码单元素名单）：VM 启动即加载，`Std` 导出的
名字在任何文件里免 `using` 可见，用户工程**不得**在 `.z42.toml` 里声明它。

**全部 stdlib 自动可见，但 namespace 要 `using` 才激活。** 每个 `z42.*` 包都随工具链分发、无条件进
编译器视野（`ScanLibsForNamespaces` / `BuildDepIndex` 对 `z42.*` 跳过 declared-deps 过滤），同 Rust
不在 `Cargo.toml` 里列 `std`。`[dependencies]` 只为第三方包而设；声明 `z42.*` 依赖纯属冗余，其中
「同 workspace 成员」子集由 workspace 构建按 **WS032** 逐条提示（文档案 `z42-toml.md` L6.2）。
（历史注：早期文档曾把这条许给从未实现的 WS013；WS032 是它落地时收窄的、可执行的版本——SDK 库
在普通工程里的按名声明是可见性种子，不提示。）

这两条合起来的后果，经常被误解：**`Console` 物理上住在 `z42.core`（`src/IO/Console.z42`），但它的
namespace 是 `Std.IO`，不在 prelude 名单里，所以 hello world 仍必须写 `using Std.IO;`**——不写报
`E0436`。「把 Console 上提到 core」已经发生，「`Std.IO` 进 prelude using」没有。

VM 找 stdlib zpkg 的顺序（`src/runtime/src/startup.rs::resolve_libs_dir`）：

1. `libs` 旋钮（`Z42_LIBS` / `--set libs=` / `[runtime].libs`）
2. `<binary-dir>/../libs/`（packages 布局）
3. `<cwd>/artifacts/intermediate/libraries/flat/{release,debug}/`（`xtask build stdlib` 的扁平视图）

每个目录都是**扁平视图**：`<包名>.zpkg`（或 `.zbc`）平铺，无 namespace 索引——VM 与嵌入宿主读每个
zpkg 的 `NSPC` 段自行建映射。

## 7. 为什么 native 声明集中不影响裁剪

`[Native]` 被降解成**按名字（字符串池下标）**的引用，native 实现在**调用期**按名解析。因此
**extern 声明在哪个 zpkg，与 native 实现模块何时加载，完全解耦**。声明集中到 `z42.core` 换来单一、
可审计的 native ABI 面，同时不牺牲按需加载。

同理，zpkg 里没有任何 target / arch / 位宽 / 字节序信息，stdlib 源码零条件编译——**所有 stdlib
zpkg（含 core）跨平台字节相同**，且**缺 builtin 的 zpkg 仍能正常加载**，调用到才报运行期错。wasm
构建把 native-interop builtin 编译掉正属此情形。

**代价**：extern 全在 core 之后，「按包有没有 extern 判定平台纯度」这个包级静态信号就没了（core
声明全部平台原语）。需要表达「这段代码零平台依赖」时改用能力清单 / 方法级标注，不靠包边界。
