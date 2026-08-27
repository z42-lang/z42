# src/libraries — z42 标准库源码

## 职责

z42 标准库的 `.z42` 源文件。每个库是独立的 z42 包，通过 `z42 xtask.zpkg build stdlib` 编译为 `.zpkg` 产物后供用户程序引用。

## 库列表

| 目录 | 包名 | 内容 |
|------|------|------|
| `z42.core/` | `z42.core` | 核心类型 + 隐式 prelude；按子目录组织：`Primitives/`（6 个 primitive 成员方法）/ `Delegates/`（callable + multicast + 订阅）/ `Protocols/`（核心接口）/ `Exceptions/`（Exception 树）/ `Collections/`（List / Dict / KVP）；根留 Object / Type / String / Convert / Assert / GC / Disposable。详见 [src/README.md](z42.core/src/README.md) |
| `z42.collections/` | `z42.collections` | 次级集合类型：`Queue`、`Stack`（未来 `LinkedList` / `SortedDictionary` / `PriorityQueue`） |
| `z42.io/` | `z42.io` | IO 应用层（纯脚本）：`FileStream` / `Stream` 家族 / `Process` / `ProcessHandle` / `Ansi` / `Stdio`（`Console`/`File`/`Directory`/`Environment`/`Path` 已上移 `z42.core`，native 语义在 core `*Native`）|
| `z42.text/` | `z42.text` | 文本处理：`StringBuilder`、`Regex` |
| `z42.encoding/` | `z42.encoding` | 字符 ↔ 字节编码：`Hex`、`Base64` (RFC 4648 §4)、`Utf8` |
| `z42.test/` | `z42.test` | 单元测试运行时（v0 imperative TestRunner；lambda 就绪后升级 v1）|
| `z42.toml/` | `z42.toml` | TOML 1.0 subset reader/writer：`TomlValue.Parse(text)` / `Stringify(root)` |
| `z42.json/` | `z42.json` | JSON RFC 8259 reader/writer：`JsonValue.Parse(text)` / `Stringify(v)` / `StringifyPretty(v)` |
| `z42.random/` | `z42.random` | Deterministic PRNG（PCG-XSH-RR）：`new Random(seed).NextInt() / NextLong() / NextDouble() / NextBool() / NextIntRange(min, max)` |
| `z42.uri/` | `z42.uri` | URI / URL parser + percent codec：`Uri.Parse(text)` / `EncodeComponent` / `DecodeComponent`（RFC 3986 子集） |
| `z42.io.binary/` | `z42.io.binary` | 二进制流读写：`BinaryReader.Read{Byte,Int16/32/64{LE,BE},Bytes,String}` / `BinaryWriter` 对称 + UTF-8 string + 自动 2x grow（namespace `Std.Binary`） |
| `z42.diagnostics/` | `z42.diagnostics` | 日志门面：`Log.{Trace,Debug,Info,Warn,Error}(msg)` + `Log.SetMinLevel(LogLevel.X)` + stderr 输出 |
| `z42.regex/` | `z42.regex` | 正则：`Regex.Compile(pat)` + `IsMatch / Find / FindAll / Replace / Split` + `Match.Group(i)`（backtracking NFA） |
| `z42.cli/` | `z42.cli` | CLI argv 解析：`ArgParser.{AddFlag, AddOption, AddPositional}` + `Parse(argv)` → `ParseResult.{GetFlag, GetOption, GetPositional, ShowHelp}` + auto `-h/--help` |
| `z42.crypto/` | `z42.crypto` | 加密原语：`Sha1` / `Sha256` (FIPS 180-4) + `HmacSha1` / `HmacSha256` (RFC 2104) + `SecureRandom` OS-CSPRNG (`GetBytes` / `NextInt` / `NextLong` / `NextU32Bounded`) |

> **两类库（别混淆）**：`src/libraries/` 同时住着**用户 stdlib**（`Std.*` 命名空间，面向应用开发者）与
> **工具链库**（`Z42.*`：`z42.ir` / `z42.project` / `z42.build`——编译器内部件下沉为共享库，供 z42c / REPL /
> z42b 复用）。下方实现规范与 [organization.md](../../docs/design/stdlib/organization.md) 的划分规则**只约束
> `Std.*` 用户库**；`Z42.*` 是编译器内部实现、不是用户 API。详见 organization.md「用户 stdlib vs 工具链库」。

## 实现规范（必须遵守）

### 1. Script-First：优先脚本实现

**尽可能把逻辑放到 `.z42` 脚本实现，减少 VM 侧的 extern / intrinsic。**

- 新增方法默认用 `.z42` 脚本实现，即使暂时性能不优
- 性能问题延后优化（profile → JIT 优化 → 必要时再下沉为 intrinsic）
- 现存 extern 逐步评估下沉：若能用"更小的 intrinsic 核 + 脚本组合"表达，
  优先迁移。例：`Contains` / `IndexOf` / `Trim` / `Substring` 已迁脚本；
  `String.Length` 等仍是 extern 的方法，在更基础原语（如 `char[]` 视图）
  就绪后也应评估是否能下沉，不把"性能担忧"当作保留 extern 的默认理由。
- 只有真正无法用脚本表达的原语才保留 extern：内存布局 / 原子指令 / 底层
  分配 / 与 VM ABI 绑定的协议方法（`Equals` / `GetHashCode` / `ToString`）。

> **判定准则（BCL/Rust 对标，2026-04-26）**：
> Runtime 提供 **primitive**（JIT 无法消除的硬能力：syscall / libm / GC barrier /
> 类型元数据 / UTF-8 codepoint 访问 / 数值字面量 parse），**feature**（集合 / 算法 /
> 格式化 / Assert / Path 字符串操作 / 算术辅助）一律脚本实现。
> 详见 [docs/design/stdlib/organization.md "Primitive vs Feature (BCL/Rust 对标)"](../../docs/design/stdlib/organization.md)。

### 2. Interop 按 native 角色两层安置（native 语义层 → core，应用层 → 纯脚本）

> **2026-08-27 refine-interop-native-separation 更新**（先后取代「VM extern 只在 core，io 例外」与
> 「io/net/threading 各作平台边界库」两版表述）。唯一 SoT：
> [docs/design/stdlib/organization.md「native 语义层 → core，应用层 → 纯脚本能力库」](../../docs/design/stdlib/organization.md)。

**每个能力拆两层：① native 语义层（`extern`/`[Native]` 原语）② 应用层（纯脚本高层 API）。按 native 角色安置：**

1. **执行基座**（io / net / threading）—— native 语义层**并入 `z42.core`**（对齐 .NET CoreLib）；
   应用层（`FileStream` / `TcpClient` / `Thread` / `Stream` 家族 / 进程编排）留 `z42.io` / `z42.net` /
   `z42.threading` **转纯脚本**、调 core 的 `*Native` 原语。`Std.IO`（Console/File/Directory/Environment/Path）、
   `Std.Time`（DateTime/…）也在 core。
2. **可插拔工具 / 算法**（compression / crypto / diagnostics / test / build）—— native + 应用整库**留独立**：
   正常执行不需要的可选插件，可独立编译、按需加载、可裁剪。
3. **运行时内核 + cross-cutting 原语**（值语义 / 反射 / GC / libm / 时钟 / 位转换）—— 本就在 `z42.core`。

**其余所有库一律纯脚本、零 interop**（collections / text / encoding / toml / json / yaml / uri / regex /
cli / random / numerics / io.binary / …），要用 native 时**通过调 core 或工具库的公开 API 间接使用**。

> **注意**：zpkg 是可移植字节码，native 引用只是"按名字在 VM **调用期**解析"的字符串，**所有 zpkg
> 本就跨平台字节相同**（core 也是），且带缺失 builtin 的 zpkg 仍能加载、调用到才报错。本规则**不是**为了
> 让 zpkg 字节相同（那已成立），而是为了：① 单一、可审计的 native ABI 面（.NET CoreLib 式）；② 服务
> 后续**按需加载 native、裁剪运行时体积**（声明位置与 native 模块加载解耦）；③ 消灭重复声明。

**配套纪律：**

- **Script-First**：逻辑尽量放脚本；interop 只提供**最小基础机制/原语**，不在 native 侧堆高层逻辑。
- **接口最小化**：interop 符号**非必要不导出**；对 interop 的包装保持**薄封装**，不叠便利方法。
- **单一声明点**：每个 native 符号在**全仓库只声明一次**。cross-cutting 原语归 core；平台能力原语归其
  能力库。（位转换 `__*_to_bits`/`__*_from_bits` → core `Std.BitConverter`、时钟 `__time_now_*` → core
  `Std.Runtime.Clock` 的多库重复声明已由 consolidate-core-intrinsics(A1) 收敛。）
- **性能升级阶梯**：**脚本实现 → 持续优化（JIT / 算法 / VM 调用机制提速）→ 仍不达标 → 才下沉为
  VM 内置实现**。VM 内置是最后手段，不是默认——优先投资"让脚本层本身更快"的通用机制。

> 目的：保持 VM / native 表面最小、可审计；stdlib 绝大部分逻辑由脚本驱动，便于自举、调试和演进。
> 新增 VM extern 视同新增语言原语，需走 vm 类型完整变更流程。

---

## 构建

```bash
z42 xtask.zpkg build stdlib         # 编译全部 lib + 扁平视图（release）
```

每次构建会：
1. 通过 workspace 模式编译每个 member → `artifacts/libraries/<lib>/dist/<lib>.zpkg`
2. **自动同步**到 VM 加载路径 `artifacts/z42/libs/<lib>.zpkg`

两个目录都已在 `.gitignore` 中，不纳入版本控制。

## 修改后

修改任意 `.z42` 源文件后重跑 `z42 xtask.zpkg build stdlib` 即可 —— 无需再手动 `package.sh` 或 `cp` 同步（这是 wave1-path-script 实施时反复踩到的坑，已修）。

---

## 未来计划（按需补齐，无空目录占位）

> 仅作 roadmap 备忘。**实际拉新包时再建目录** — 避免半成品占位包污染构建。

### 已规划但未启动

| 包 | 内容 | 阶段 | 触发条件 |
|----|------|------|---------|
| `z42.diagnostics` | `Debug` / `Trace` / `Stopwatch` / `Assert*` 扩展 | L2 | 性能调优 / 调试日志需求出现 |
| `z42.threading` | `Thread` / `Mutex` / `Atomic*` / `Channel` | L3 | 并发模型设计完成（`Rc<RefCell>` → `Arc<Mutex>` 配套）|
| `z42.async` | `Task<T>` / `async`/`await` runtime / `ValueTask` | L3 | 关键字 `async` / `await` parser 完成 |
| `z42.net` | `Socket` / `HttpClient` / `Url` | L3+ | 异步运行时就绪后 |
| `z42.json` | `JsonReader` / `JsonWriter` / `JsonNode` | L3+ | 反射 (L3-R) 完成（自动序列化）|
| ~~`z42.test`~~ | ✅ v0 已落地（imperative `TestRunner`）—— v1 等 lambda、v2 等 [Test] attribute + reflection | — | 2026-04-27 |
| `z42.linq` | `Where` / `Select` / `OrderBy` 扩展（基于 `IEnumerable<T>`）| L3 | Lambda + IEnumerable codegen 升级 |
| `z42.numerics` | `BigInteger` / `Complex` / 矩阵基础 | L3 | 数值计算需求 |
| `z42.crypto` | 哈希 / 对称加密 / 签名（封装 native 库）| L3+ | 安全场景需求 |
| `z42.compression` | gzip / zstd 封装 | L3+ | 文件 / 网络压缩需求 |

### 既有包的扩展计划

| 包 | 待补齐 |
|----|--------|
| `z42.core` | `Nullable<T>` 显式类型（暂用语言级 `T?`，独立类型留待系统设计）<br>`KeyValuePair<K,V>`（Dictionary 实现 `IEnumerable` 需要）<br>`Range` / `Index`（C# 8 风切片）<br>`Tuple<...>`（多返回值；当前 z42 无 tuple 类型）|
| `z42.collections` | `LinkedList<T>` / `SortedDictionary<K,V>` / `PriorityQueue<T>` / `ImmutableArray<T>`<br>List / Dictionary 实现 `IEnumerable<T>`（端到端 foreach IEnumerator 路径）|
| `z42.io` | `Stream` / `BufferedStream` / `MemoryStream`<br>`TextReader` / `TextWriter` 抽象类<br>`Directory` / `FileInfo` / `DirectoryInfo`<br>`Encoding` (UTF-8 / UTF-16)|
| `z42.text` | `Encoding` 体系（与 `z42.io` 协调）<br>`StringReader` / `StringWriter`<br>`Regex` 完整实现（当前占位）|

### 跨包 backlog

- `IComparer<T>` / `IEqualityComparer<T>` 接入 List.Sort / Dictionary ctor 重载
- `IEnumerable<T>` 接入 foreach codegen（当前 codegen 仅识别 Count + get_Item 鸭子协议）
- `IFormattable` 接入 `string.Format` / `$"{x:format}"` 字符串插值格式说明符
- 通用 generic interface dispatch 修复（TypeChecker 不识别 `IComparer<int>` 等的 TypeArgs，阻塞接口变量直接调用）

### 不规划做的（明确否决）

- ❌ "完整 BCL 移植"：z42 仅取 C# BCL **常用 80%**，避开 LINQ-to-SQL / WPF / WCF / Remoting 等历史包袱
- ❌ Reflection-heavy 序列化（XmlSerializer 等）：等 L3-R 反射完成后再考虑，且只做 JSON
- ❌ AppDomain / 卸载：与 z42 lazy-loader 模型不契合
- ❌ 静态类反射创建（`Activator.CreateInstance`）：等 L3-R

---

## Extern 现状审计表

> **2026-04-26 起维护**。每次 stdlib 改动起手必看；新增 extern 必须在 PR 描述里
> 回答"BCL/Rust 把它当 primitive 吗？" —— 回答不出 → 拒绝。
>
> **状态枚举**：
> - 🟢 **Primitive 必须保留** —— BCL/Rust 同样是 intrinsic / extern / syscall
> - 🟡 **Wave 1 待迁** —— 纯脚本可表达，无需新基础设施
> - 🔵 **Wave 2 待迁** —— 走 codegen 特化（不是脚本，是 IR 直降）
> - ⚫ **Wave 3 待迁** —— 需要先补一个底层原语
> - ❌ **Dead code 待删** —— 编译器已不 emit

### Wave 0（dead code）— 13 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__list_new` / `__list_add` / `__list_remove_at` / `__list_contains` / `__list_clear` / `__list_insert` / `__list_sort` / `__list_reverse` (8) | ❌ | L3-G4h step3 后 List<T> 已纯脚本 atop `T[]`，编译器不再 emit |
| `__dict_new` / `__dict_contains_key` / `__dict_remove` / `__dict_keys` / `__dict_values` (5) | ❌ | 同上，Dictionary<K,V> 已纯脚本 |

### I/O — 6 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__println` / `__print` / `__readline` | 🟢 | host FFI（OS stdout/stdin） |
| `__concat` | 🟡 | 候选走 codegen 特化为 IR 字符串拼接 |
| `__len` | 🟢 | 通用长度（数组 / 字符串），UTF-8 byte vs char 由 VM 决定 |
| `__contains` | 🟡 | 字符串 / 列表通用，可拆为 per-type 脚本实现 |

### String — 10 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__str_length` / `__str_char_at` / `__str_from_chars` | 🟢 | UTF-8 codepoint 访问，BCL `string.Length` / Rust `str::chars` 同级 |
| ~~`__str_split` / `__str_join` (2)~~ | ✅ 已删 | 2026-04-27 wave1-string-script — 脚本基于 `CharAt` + `Substring` 两遍扫描 |
| ~~`__str_concat`~~ | ✅ 已删 | 2026-04-27 wave3a-str-concat-script — `Std.String.Concat` 用 `+` 即 IR StrConcatInstr |
| ~~`__str_format`~~ | ✅ 已删 | 2026-04-27 wave3b-str-format-script — `Std.String.Format` 用链式 `Replace` + `Convert.ToString`。原计划等 IFormattable，实测无需（builtin 只做 `{0}` 字面替换） |
| ~~`__str_to_string`~~ | ✅ 已删 | 2026-08-27 shrink-primitive-native-interop — 旧 builtin 只是原样返回自身，迁脚本 `return this;` |
| `__str_equals` / `__str_hash_code` / `__str_compare_to` | 🟢 | Object 协议方法（`__str_equals` 类型宽容处理 null/装箱），VM ABI 绑定 → 保留 |

### Char — 3 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__char_is_whitespace` | 🟢 | Rust `char::is_whitespace()` 真 Unicode 分类，脚本无法等价 → 保留 |
| ~~`__char_to_lower` / `__char_to_upper` (2)~~ | ✅ 已删 | 2026-08-27 shrink-primitive-native-interop — 旧实现是 `to_ascii_lowercase/uppercase`（纯 ASCII，非 Unicode），迁 ASCII 脚本见 [Primitives/Char.z42](z42.core/src/Primitives/Char.z42) |

### Convert / Parse — 4 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__int_parse` / `__long_parse` / `__double_parse` | 🟢 | Rust 数值解析；BCL `int.Parse` / Rust `str::parse` 同级 |
| `__to_str` | 🟢 | 通用动态值 → 字符串，VM 元数据依赖 |

### Primitive 协议 — 17 项

| Builtin | 状态 | 备注 |
|---|---|---|
| ~~`__int_compare_to` / `__double_compare_to` / `__char_compare_to` (3)~~ | ✅ 已删 | 2026-04-27 wave2-compare-to-script — 5 个 primitive (int/long/double/float/char) 的 `CompareTo` 全脚本：`if (this < other) return -1; if (this > other) return 1; return 0;` 用 IR `<`/`>`，与 Rust `partial_cmp.unwrap_or(0)` 等价（NaN → 0 自然落到 return 0）|
| ~~`__int32_equals` / `__int32_hash_code` (2)~~ | ✅ 已删 | 2026-08-27 shrink-primitive-native-interop — 8 个整型（Int32/Int16/SByte/Byte/UInt16/UInt32/Int64/UInt64）共享；`Equals`→`this == other`，`GetHashCode`→窄型 `(int)this` / 64 位折叠 `(int)(v ^ (v>>32))`（顺带修 long 截断 bug）|
| `__int32_to_string` (1) | 🟢 | 整数十进制格式化，纯脚本 digit-loop 是热路径回归 → 保留 |
| ~~`__double_equals` / `__double_hash_code` (2)~~ | ✅ 已删 | 2026-08-27 shrink-primitive-native-interop — Double/Single；`Equals`→`this == other`，`GetHashCode` 经 BitConverter 折叠 IEEE-754 位模式 |
| `__double_to_string` (1) | 🟢 | 浮点最短往返（Ryū 级）纯脚本不现实 → 保留 |
| ~~`__bool_equals` / `__bool_hash_code` / `__bool_to_string` (3)~~ | ✅ 已删 | 2026-04-27 wave1-bool-script — 脚本实现见 [z42.core/src/Bool.z42](z42.core/src/Bool.z42)，`ToString` 输出 `"true"/"false"` 小写（与 Rust 一致）|
| ~~`__char_equals` / `__char_hash_code` (2)~~ | ✅ 已删 | 2026-08-27 shrink-primitive-native-interop — `Equals`→`this == other`，`GetHashCode`→`(int)this` |
| `__char_to_string` (1) | 🟢 | 保留 native（char→单字符 string）|
| `__str_compare_to`（已计入 String 区）| — | — |

### Assert — 0 项（2026-04-27 wave1-assert-script 全部迁出）

| Builtin | 状态 | 备注 |
|---|---|---|
| ~~`__assert_eq` / `__assert_true` / `__assert_false` / `__assert_null` / `__assert_not_null` / `__assert_contains`~~ | ✅ 已删 | 脚本实现见 [z42.core/src/Assert.z42](z42.core/src/Assert.z42) — 纯 `if (!cond) throw new Exception(...)`，与 BCL/Rust 一致 |

### Math — 15 项

| Builtin | 状态 | 备注 |
|---|---|---|
| ~~`__math_abs` / `__math_max` / `__math_min` (3)~~ | ✅ 已删 | 2026-04-27 wave1-math-script — int + double overload 脚本，见 [z42.core/src/Math.z42](z42.core/src/Math.z42) |
| `__math_pow` / `__math_sqrt` / `__math_log` / `__math_log10` / `__math_sin` / `__math_cos` / `__math_tan` / `__math_atan2` / `__math_exp` (9) | 🟢 | libm FPU 指令，BCL/Rust 都是 extern |
| `__math_floor` / `__math_ceiling` / `__math_round` (3) | 🟢 | libm 一致性；技术上脚本可表达，但保 libm 行为以匹配 BCL/Rust |

### File / Path / Env — 14 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__file_read_text` / `__file_write_text` / `__file_append_text` / `__file_exists` / `__file_delete` (5) | 🟢 | syscall，BCL/Rust 同级 |
| ~~`__path_join` / `__path_get_extension` / `__path_get_filename` / `__path_get_directory` / `__path_get_filename_without_ext` (5)~~ | ✅ 已删 | 2026-04-27 wave1-path-script — Unix `/` 语义脚本，见 [z42.io/src/Path.z42](z42.io/src/Path.z42) + golden test [16_path](z42.io/tests/16_path/)。`Path.Separator` 常量待静态字段访问支持（L3+）|
| `__env_get` / `__env_args` / `__process_exit` / `__time_now_ms` (4) | 🟢 | syscall / process state |

### Object 协议 — 5 项

| Builtin | 状态 | 备注 |
|---|---|---|
| `__obj_get_type` / `__obj_ref_eq` / `__obj_hash_code` / `__obj_equals` / `__obj_to_str` | 🟢 | VM 类型元数据，BCL `RuntimeHelpers` / Rust `TypeId` 同级 |

### 汇总

| Wave | 数量 | 处置 |
|---|---|---|
| Wave 0（dead code）| 13 | ✅ 已完成（extern-audit-wave0）|
| Wave 1（feature → 脚本）| 19 | ✅ 全部完成（assert 6 + bool 3 + math 3 + path 5 + str split/join 2）|
| Wave 2（codegen 特化 → 实际纯脚本）| 3 | ✅ 完成（wave2-compare-to-script）。原计划 codegen 特化，实测 `<`/`>` 走 IR cmp+jmp 已足够 |
| Wave 3a（str_concat → 脚本）| 1 | ✅ 完成（wave3a-str-concat-script），原计划 codegen，实测 `+` 已是 IR 指令 |
| Wave 3b（str_format → 脚本）| 1 | ✅ 完成（wave3b-str-format-script），用 Replace + Convert.ToString 替代；IFormattable 等真正需要格式说明符再独立 spec |
| Wave 4（primitive Eq/Hash/casing/str-ToString → 脚本）| 9 | ✅ 完成（shrink-primitive-native-interop）。纠正「Object 协议成员必须保留」的错误判据——判据应是「native 实现是否平凡」（bool 早已迁脚本即先例）。删 `__int32_equals`/`__int32_hash_code`/`__double_equals`/`__double_hash_code`/`__char_equals`/`__char_hash_code`/`__char_to_lower`/`__char_to_upper`/`__str_to_string` |
| 🟢 Primitive 必须保留 | ~33 | ToString/Parse/UTF-8 intrinsic/libm/BitConverter/Object 协议/反射 等，与 BCL/Rust 标杆一致 |
| **当前总计** | **~34** | 从 ~43 再降 9（Wave 4）|

### Wave 进度

| Wave | 状态 | 完成日期 |
|---|---|---|
| Wave 0 | ✅ 已完成（extern-audit-wave0）| 2026-04-26 |
| Wave 1.1 Assert | ✅ 已完成（wave1-assert-script）| 2026-04-27 |
| Wave 1.2 Bool 三件套 | ✅ 已完成（wave1-bool-script）| 2026-04-27 |
| Wave 1.3 Math abs/max/min | ✅ 已完成（wave1-math-script）| 2026-04-27 |
| Wave 1.4 Path 五件套 | ✅ 已完成（wave1-path-script）| 2026-04-27 |
| Wave 1.5 String split/join | ✅ 已完成（wave1-string-script）| 2026-04-27 |
| Wave 2 | ✅ 已完成（wave2-compare-to-script）| 2026-04-27 |
| Wave 3a str_concat | ✅ 已完成（wave3a-str-concat-script）| 2026-04-27 |
| Wave 3b str_format | ✅ 已完成（wave3b-str-format-script）| 2026-04-27 |
| Wave 4 primitive Eq/Hash/casing/str-ToString | ✅ 已完成（shrink-primitive-native-interop）| 2026-08-27 |
