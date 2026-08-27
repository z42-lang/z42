# refine-interop-native-separation — native 语义层归 core + 应用层纯脚本化

> 状态：DRAFT（2026-08-27）
> 类型：docs（规范细化，本次）+ 后续 refactor（语义上移 / 库转纯脚本 / 删薄包）
> 子系统：stdlib / docs
> SoT 落点：`docs/design/stdlib/organization.md`「平台边界库 vs 全平台共享库」节

## What / Why

把 interop 归属按 **native 角色两层分解** 明确成文，让今后有据可依：

- **两层分解**：每个能力拆 ① **native 语义层**（`extern`/`[Native]` 原语）+ ② **应用层**（纯脚本高层 API 类）。
- **安置**：
  - **执行基座**（io/net/threading）：native 语义层**并入 `z42.core`**（对齐 .NET CoreLib）；应用层
    留 z42.io/net/threading **转纯脚本**；抽走语义后剩余逻辑很薄的库 → **删包**、逻辑并入 core。
  - **可插拔工具/算法**（diagnostics / test / build / compression / crypto）：整库（语义+应用）**留独立**——
    正常执行不需要，可独立编译、按需加载、可裁剪。diagnostics 是样板。
  - **运行时内核**（值语义/反射/GC/libm）：本就在 core。
- **根本动因**：native 语义集中 core 拿到**单一可审计 native ABI 面**，**同时不牺牲**「按需加载 native、
  减少运行时体积」——因 native 按名在**调用期**惰性解析（已验证：`exec_call.rs` Builtin 派发、缺名报
  `unknown builtin`、JIT 侧函数首次编译时解析；wasm 直接编译掉 native-interop builtin），**声明位置与
  native 模块加载时机解耦**。代价：失去"按包有无 extern 判定平台纯度"的包级信号，改用能力清单/属性表达。
- **.NET 关系 —— 对齐**：io/threading 语义入 core = 对齐 CoreLib；compression/crypto/diagnostics 独立 =
  对齐独立程序集。z42 额外把应用层从语义层剥离为纯脚本（.NET 无此分层）。

## 梳理（当前 stdlib 按新模型分类）

| 归属 | 库 / 符号 | 说明 |
|------|-----------|------|
| **core：运行时内核**（已在） | primitive parse/equals/hash、string/char、反射（`__customAttributes`/`MakeGenericMethod`/`Clone`）、GCHandle、libm math | VM 对象模型 / 值语义 / 反射内核 |
| **→ core：执行基座语义层**（本次迁移） | io 的 `_File*`/Console/Env/Process 原语（~62）、net 的 socket/TLS/DNS 原语（~38）、threading 的 Thread/Mutex/Channel/Atomic 原语（~25） | OS 语义原语上移 core；库体转纯脚本应用层 |
| **→ core：跨切反射原语** | json：`NewArray`/`ArrayGet`/`ArraySet`/`ArrayLength`/`PropAttrs` | 通用反射+动态数组，误漏进 json；先核对是否与 #278 `Std.Array`/`PropertyInfo` 重复 |
| **纯脚本应用层**（语义上移后） | z42.io / z42.net / z42.threading 的高层 API 类（`File`/`Path`/stream/`TcpClient`/`Thread`…） | 调 core 原语；逻辑薄的库 → 删包并入 core |
| **独立工具/算法库**（整库留） | compression / crypto / diagnostics（Heap+Log）/ test / build | 可插拔、按需加载、可裁剪 |
| **已纯脚本** | io.binary / ir / text / random / numerics / collections / encoding / time / toml / yaml / uri / regex / cli | 纯计算 |

**开放项**：compression / crypto 暂归"独立工具/算法库"（对齐 .NET 独立程序集，属可选插件）——待 User 确认
是否与 io/net/thread 一样只把语义上移 core、还是整库留独立。

## 配套结构惯例（.NET `Interop.*` 照搬）

留在独立库的 native 把 `[Native]` extern **收进单一 `Native.z42` / sink 文件**，不散落在业务文件。
现状：io/net/threading 已部分落地（Process/Dns/Thread/Channel/Mutex/RwLock）——语义上移 core 后，这些 sink
连同原语一并进 core；compression/crypto/diagnostics/test 的 sink 待收敛。

## Tasks

1. ✅ **[docs]** 两层分解 + 安置规则 + 按需加载动因 + .NET 对齐 + 调用期解析事实校正写入
   `organization.md`（TL;DR #3、①/②/③ 分类表、判据节、澄清段、现状表、L2/R3 章节）；`libraries/README.md §2`
   摘要同步；`time.md`/`overview.md` 迁移 banner。
2. ✅ **[code]** io/net/threading 的 native 语义层原语上移 core；三库转纯脚本应用层：
   - threading：4 个 `*Native` → `core/src/Native/ThreadingNative.z42`。
   - net：4 个 `*Native` → `core/src/Native/NetNative.z42`。
   - io：Console/ConsoleError/File/Directory/Environment/Path 整搬 `core/src/IO/`（.NET CoreLib 对齐）；
     FileStream/Process/ProcessHandle 的 native 抽 `core/src/IO/{FileStreamNative,ProcessNative}.z42`；
     io 现零 extern。测试留 z42.io（迁 core 作 follow-up）。
   - 格式面零 bump。⚠️ 触发 bootstrap-seed 轴④——**冷启动 + 字节不动点交 CI**（本机 seed 太旧、z42vm 挂起，本地只能 workspace 编译验证 21/24 库全绿）。
3. ✅ **[code]** `z42.time` 纯脚本类型（DateTime/…）整体下沉 `core/src/Time/`，**删 z42.time 包**（User 决策，.NET CoreLib 对齐）；4 依赖库 toml + workspace default-members + test skip-rule + tests/bench 同步。
4. ✅ **[核对]** json 反射（NewArray/ArrayGet/…）——**已于 #278/#284 下沉 core**（`Std.Array` + `Reflection.PropertyInfo`），json 现零 extern，无需再动。
5. ⏳ **[follow-up]** compression/crypto/diagnostics/test 的 extern 收进各自 `Native.z42` sink（独立库内组织，非搬 core）。
6. ⏳ **[follow-up]** io 测试迁 core/tests；现存重复声明收敛（`__double_to_bits` io.binary+ir；`__time_now_*`）——同源 A1。

## 验证

- **本地（种子墙允许范围内）**：workspace 全量重建 21/24 库全绿、零 undefined；threading 端到端编译验证；net 与 threading 同机制等价；time 4 消费方编译验证；io + 全部 File/Console/Path/DateTime 下游消费方编译验证。
- **CI 权威**（本地不可验）：`xtask test` 全测试 + **自举字节不动点（gen1==gen2）** + cold-start。核心增肥但**零格式 bump**，[[two-gen-bootstrap-regressed]] 的格式-bump 回归不应触发。
- z42.build 的 `[Record]` 编译错是本机 seed 太旧（落后 origin/main），与本改动无关，abort 挡住 net/z42.build 本地编译。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
