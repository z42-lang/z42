# 标准库的实现分层与 native 预算

> 对齐：2026-09-17 ｜ 代码：`src/libraries/`（z42 源）、`src/runtime/src/corelib/`（builtin 实现）、
> `src/compiler/z42c.semantics/src/StubEmitter.z42`（`[Native]` 分流）
>
> 包怎么划分、新包怎么开 → [包划分与依赖层级](organization.md)。
> builtin 表结构 / `BuiltinId` 不变式 / typed native 通路 → [native ABI](../runtime/native-abi.md)。
> 用户怎么调这些 API → 参考手册 [标准库](../../../reference/src/stdlib/README.md)。

本页写**一个 stdlib 方法该落在哪一层实现**、全仓 native 表面今天有多大、以及新增一个 builtin 要改哪几处。

## 1. 实际只有两层半

```
┌──────────────────────────────────────────────────────────┐
│  脚本层   src/libraries/*.z42                            │
│  绝大多数 stdlib 逻辑；编译成 .zpkg，与用户代码同等对待   │
├──────────────────────────────────────────────────────────┤
│  codegen 特化   z42c.semantics/{Operator,Expr}Emitter    │
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
| **fs backend** | native `std::fs` ↔ wasm 内存 VFS 的运行期切换 | `corelib/fs_backend/{native,memory}.rs` |

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

1. `src/runtime/src/corelib/<area>.rs` —— 写 `fn(&VmContext, &[Value]) -> Result<Value>`。
2. `src/runtime/src/corelib/builtin_table_ext.rs` —— **只能表尾追加**（下标即 `BuiltinId`，会被烤进
   zbc，中间插入会让既有产物整体错位）。
3. `src/libraries/z42.core/src/…` —— 写 `[Native("__…")] public static extern` 声明；归属见
   [包划分](organization.md)。

没有第四处：z42c 不维护 builtin 签名表，签名就是那条 z42 声明本身。命名约定
（`__<area>_<verb>`）与 area 清单在 [native ABI §6.2](../runtime/native-abi.md)。

## 5. 全仓 native 表面（实测）

VM 注册表 `BUILTINS`（`builtin_table.rs` + `builtin_table_ext.rs`）共 **321** 个条目。stdlib 侧
`[Native("__…")]` 声明的**不同符号** **288** 个，全部能在表里找到。

按包分布——**全仓只有五个包声明 interop**：

| 包 | 声明文件数 | 不同 `__` 符号 | 性质 |
|---|---|---|---|
| `z42.core` | 35 | 275 | 语义汇聚核 |
| `z42.test` | 3 | 8 | 可插拔工具（harness / 计时 / 模块加载） |
| `z42.diagnostics` | 2 | 3 | 可插拔工具（heap 内省 / 计数器） |
| `z42.scripting` | 1 | 3 | 可插拔工具（模块加载） |
| `z42.compression` | 8 | 12 个 `entry =`（`lib = "z42_compression"`） | 可插拔工具（外部 cdylib） |

compression 那 12 个**不在** `BUILTINS` 里——它们由 cdylib 在加载时注册进 per-VM 的 ext 表
（见 [native ABI §6.3](../runtime/native-abi.md)）。

其余 **全部为零**：`io` / `net` / `threading` / `json` / `text` / `collections` / `encoding` /
`toml` / `yaml` / `uri` / `regex` / `cli` / `random` / `numerics` / `crypto` / `build` / `project`
/ `ir` / `z42c.core` / `z42c.syntax`。`z42.crypto` 的全部哈希、HMAC、椭圆曲线都是纯 z42。

`z42.core` 内部 275 个符号的来源（前几名）：

| 文件 | 符号数 | 领域 |
|---|---|---|
| `Native/NetNative.z42` | 34 | TCP / UDP / TLS / DNS |
| `Type.z42` | 33 | 反射类型对象 |
| `Math.z42` | 20 | libm |
| `IO/File.z42` | 18 | 文件 syscall |
| `IO/ProcessNative.z42` | 12 | 子进程 |
| `GC/GC.z42` | 12 | GC 控制与内省 |
| `String.z42` | 11 | UTF-8 访问 |

### 表里的死条目

有 **33** 个表项没有任何 stdlib 声明。其中一部分是编译器直接发射的（`__concat` / `__len` /
`__contains` / `__box_prim` / `__box_struct` / `__methodof` / `__struct_hash_code`），一部分住在
`src/toolchain/`（`__repl_*` / `__vfs_*`），剩下一族是**真死码**：

- `__mutex_new` / `__mutex_lock_acquire` / `__mutex_unlock` / `__mutex_store`
- `__rwlock_*`（9 个）
- `__channel_*`（6 个）

`Mutex<T>` / `RwLock<T>` / `Channel<T>` 自 `store-sync-values-in-heap` 起改为纯 z42 实现，只用
`MonitorNative`（`New` / `Enter` / `TryEnter` / `Exit` / `Wait`）这一组原语——把值放回普通字段让 GC 看得见，
此前存在 Rust 侧 `parking_lot::Mutex<Value>` 里的值没有 GC 根覆盖。旧的那 19 个 builtin 从此无人
声明。因为下标就是 `BuiltinId`，**它们不能删，只能留着占位**。

> 审计时注意：`grep "extern"` 会大量命中注释——`z42.io` / `z42.net` / `z42.json` / `z42.text` 里
> 提到 extern 的行全是「本文件不再自带 extern，已上移 core」这类说明。要数真实声明必须匹配
> `[Native(` 或带修饰符的 `extern` 声明行。

## 6. 加载与可见性

**`z42.core` 是隐式 prelude**（`DepScan.IsPrelude`，硬编码单元素名单）：VM 启动即加载，`Std` 导出的
名字在任何文件里免 `using` 可见，用户工程**不得**在 `.z42.toml` 里声明它。

**全部 stdlib 自动可见，但 namespace 要 `using` 才激活。** 每个 `z42.*` 包都随工具链分发、无条件进
编译器视野（`ScanLibsForNamespaces` / `BuildDepIndex` 对 `z42.*` 跳过 declared-deps 过滤），同 Rust
不在 `Cargo.toml` 里列 `std`。`[dependencies]` 只为第三方包而设；非 `z42.*` 工程声明 `z42.*` 依赖 →
`WS013`。

这两条合起来的后果，经常被误解：**`Console` 物理上住在 `z42.core`（`src/IO/Console.z42`），但它的
namespace 是 `Std.IO`，不在 prelude 名单里，所以 hello world 仍必须写 `using Std.IO;`**——不写报
`E0436`。「把 Console 上提到 core」已经发生，「`Std.IO` 进 prelude using」没有。

VM 找 stdlib zpkg 的顺序（`src/runtime/src/startup.rs::resolve_libs_dir`）：

1. `libs` 旋钮（`Z42_LIBS` / `--set libs=` / `[runtime].libs`）
2. `<binary-dir>/../libs/`（packages 布局）
3. `<cwd>/artifacts/build/libraries/dist/{release,debug}/`（`xtask build stdlib` 的扁平产出）
4. `<cwd>/artifacts/z42/libs/`

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
声明全部平台原语）。需要表达「这段代码零平台依赖」时改用能力清单 / 方法级标注，不再靠包边界。
