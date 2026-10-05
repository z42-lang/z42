# src/libraries — z42 标准库源码

## 职责

z42 标准库的 `.z42` 源文件。每个库是独立的 z42 包，通过 `xtask build stdlib` 编译为 `.zpkg` 产物后供用户程序引用。

## 库列表

| 目录 | 内容 |
|------|------|
| `z42.core/` | 核心类型 + 隐式 prelude（Object / String / primitive / 集合三件套 / 异常 / 委托 / 反射 / 时间 / GC / 断言 `Assert`，以及 io·net·threading 的 native 语义层）。详见 [z42.core/src/README.md](z42.core/src/README.md) |
| `z42.collections/` | 次级集合：`Queue` / `Stack` / `LinkedList` / `SortedSet` / `PriorityQueue` |
| `z42.io/` | IO 应用层：`Stream` 家族 / `TextReader`·`TextWriter` / `Process` / `Ansi` / 二进制读写 `Std.IO.Binary` |
| `z42.text/` | `StringBuilder` / `Levenshtein` / `Strings` |
| `z42.encoding/` | `Hex` / `Base64` / `Base64Url` / `Base32*` / `Utf8` / `Utf16` / `Utf32` |
| `z42.test/` | 测试运行时：`TestIO` / `Bencher` / `TestRunner` / `Runner` / `TestReport` |
| `z42.toml/` `z42.json/` `z42.yaml/` | TOML / JSON（含对象 serde）/ YAML 子集 reader·writer |
| `z42.uri/` | URI 解析 + percent codec（RFC 3986 子集） |
| `z42.regex/` | 回溯式正则（`Regex.Compile` / `Find` / `Replace` / `Split`） |
| `z42.cli/` | argv 解析（`ArgParser` / `SubcommandRouter`） |
| `z42.random/` | 确定性 PRNG（PCG-XSH-RR） |
| `z42.numerics/` | `BigInt` / `Complex` / `Decimal` |
| `z42.crypto/` | 摘要 / MAC / KDF / AES·ChaCha20 / 签名·密钥交换 / `SecureRandom` |
| `z42.compression/` | Gzip / Zlib / Deflate / Zstd / Brotli / LZ4 + Tar / Zip（native 在独立 cdylib） |
| `z42.net/` | TCP / UDP / DNS / TLS / HTTP / WebSocket |
| `z42.threading/` | `Thread` / `Mutex<T>` / `RwLock<T>` / `Channel<T>` / `Timer` |
| `z42.diagnostics/` | 日志 `Log` + 堆保留诊断 `Heap` + 运行时计数 `RuntimeStats` |

各库详情见各自目录的 `README.md`；工作区清单为 `z42.workspace.toml`。

## 实现规范（必须遵守）

### 1. Script-First：优先脚本实现

**尽可能把逻辑放到 `.z42` 脚本实现，减少 VM 侧的 extern / intrinsic。**

- 新增方法默认用 `.z42` 脚本实现，即使性能暂不是最优
- 性能问题延后优化（profile → JIT 优化 → 必要时再下沉为 intrinsic）
- 现存 extern 逐步评估下沉：若能用"更小的 intrinsic 核 + 脚本组合"表达，
  优先迁移。例：`Contains` / `IndexOf` / `Trim` / `Substring` 已迁脚本；
  `String.Length` 等仍是 extern 的方法，在更基础原语（如 `char[]` 视图）
  就绪后也应评估是否能下沉，不把"性能担忧"当作保留 extern 的默认理由。
- 只有真正无法用脚本表达的原语才保留 extern：内存布局 / 原子指令 / 底层
  分配 / 与 VM ABI 绑定的协议方法（`Equals` / `GetHashCode` / `ToString`）。

> **判定准则（BCL/Rust 参照）**：
> Runtime 提供 **primitive**（JIT 无法消除的硬能力：syscall / libm / GC barrier /
> 类型元数据 / UTF-8 codepoint 访问 / 数值字面量 parse），**feature**（集合 / 算法 /
> 格式化 / Assert / Path 字符串操作 / 算术辅助）一律脚本实现。
> 详见 [docs/internals/src/stdlib/organization.md "Primitive vs Feature (BCL/Rust 对标)"](../../docs/internals/src/stdlib/organization.md)。

### 2. Interop 按 native 角色两层安置（native 语义层 → core，应用层 → 纯脚本）

> 唯一 SoT：
> [docs/internals/src/stdlib/organization.md「native 语义层 → core，应用层 → 纯脚本能力库」](../../docs/internals/src/stdlib/organization.md)。

**每个能力拆两层：① native 语义层（`extern`/`[Native]` 原语）② 应用层（纯脚本高层 API）。按 native 角色安置：**

1. **执行基座**（io / net / threading）—— native 语义层**并入 `z42.core`**；
   应用层（`FileStream` / `TcpClient` / `Thread` / `Stream` 家族 / 进程编排）留 `z42.io` / `z42.net` /
   `z42.threading` **转纯脚本**、调 core 的 `*Native` 原语。`Std.IO`（Console/File/Directory/Environment/Path）、
   `Std.Time`（DateTime/…）也在 core。
2. **可插拔工具 / 算法**（compression / crypto / diagnostics / test / build）—— native + 应用整库**留独立**：
   正常执行不需要的可选插件，可独立编译、按需加载、可裁剪。**例外：OS 熵原语**
   `__crypto_random_bytes` 不是「crypto 算法」而是 OS 能力（同 `__time_now_*`），归 ③ 的
   cross-cutting 原语，落 core `Std.Runtime.Entropy`——
   core 的 `Guid.NewGuid` 需要它、prelude 不能反依赖 crypto。crypto 的**算法**（哈希 / HMAC）仍留本类
   可裁剪；`SecureRandom` 作安全语义门面留 crypto、委托 core 熵原语。
3. **运行时内核 + cross-cutting 原语**（值语义 / 反射 / GC / libm / 时钟 / 位转换 / **OS 熵**）—— 本就在 `z42.core`。

**其余所有库一律纯脚本、零 interop**（collections / text / encoding / toml / json / yaml / uri / regex /
cli / random / numerics / io.binary / …），要用 native 时**通过调 core 或工具库的公开 API 间接使用**。

> **注意**：zpkg 是可移植字节码，native 引用只是"按名字在 VM **调用期**解析"的字符串，**所有 zpkg
> 本就跨平台字节相同**（core 也是），且带缺失 builtin 的 zpkg 仍能加载、调用到才报错。本规则**不是**为了
> 让 zpkg 字节相同（那已成立），而是为了：① 单一、可审计的 native ABI 面；② 服务
> 后续**按需加载 native、裁剪运行时体积**（声明位置与 native 模块加载解耦）；③ 消灭重复声明。

**配套纪律：**

- **Script-First**：逻辑尽量放脚本；interop 只提供**最小基础机制/原语**，不在 native 侧堆高层逻辑。
- **接口最小化**：interop 符号**非必要不导出**；对 interop 的包装保持**薄封装**，不叠便利方法。
- **单一声明点**：每个 native 符号在**全仓库只声明一次**。cross-cutting 原语归 core；平台能力原语归其
  能力库。（位转换 `__*_to_bits`/`__*_from_bits` → core `Std.BitConverter`、时钟 `__time_now_*` → core
  `Std.Runtime.Clock`、OS 熵 `__crypto_random_bytes` → core `Std.Runtime.Entropy`，
  `z42.crypto.SecureRandom` 委托 core 而不重声明。）
- **性能升级阶梯**：**脚本实现 → 持续优化（JIT / 算法 / VM 调用机制提速）→ 仍不达标 → 才下沉为
  VM 内置实现**。VM 内置是最后手段，不是默认——优先投资"让脚本层本身更快"的通用机制。

> 目的：保持 VM / native 表面最小、可审计；stdlib 绝大部分逻辑由脚本驱动，便于自举、调试和演进。
> 新增 VM extern 视同新增语言原语，需走 vm 类型完整变更流程。

---

## 构建与测试

```bash
xtask build stdlib              # 编译全部 lib + 扁平视图（release）
xtask test stdlib [lib]         # 跑全部（或单个库的）[Test]
```

构建通过 workspace 模式编译每个 member 并自动同步到 VM 加载路径；产物目录不纳入版本控制。
修改任意 `.z42` 后重跑 `xtask build stdlib` 即可。

## 关联文档

- stdlib 组织规则（Primitive vs Feature、native 角色分层）：[docs/internals/src/stdlib/organization.md](../../docs/internals/src/stdlib/organization.md)
- 库 API 参考：[docs/reference/src/stdlib/](../../docs/reference/src/stdlib/README.md)
- `[Native("__x")]` 声明 ↔ VM `BUILTINS` 表的对账：`src/runtime/src/corelib/native_decl_tests.rs`（新增 extern 须在 stdlib 声明且单一声明点，视同新增语言原语，走 vm 类型完整变更流程）
