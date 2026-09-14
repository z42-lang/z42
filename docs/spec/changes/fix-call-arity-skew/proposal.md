# Proposal: 普通方法调用在版本 skew 下命中错签名时不再照常执行

> **状态：📝 DRAFT v2（判据已裁决 = 方案 C；待确认实施）** | 创建：2026-09-14
> 前置：`fix-ctor-arity-skew`（#620，构造器站点 ⑤）、`encode-ctorless-objnew`（#629）已合并。
> 本 change 把站点 ⑤ 的判定推广到**普通方法调用**——`dep-version-skew-program` 记为「另量」的那一项。

## Why —— 实测复现（`06aa65701`，未改动代码）

`src/tests/cross-zpkg/` 新增 4 条 fixture，interp 与 jit **结果一致**：

| fixture | 形态 | 实测 |
|---|---|---|
| `call_arity_instance_skew` | v2 `Label()` / v1 `Label(string)`，普通类 ⇒ `VCall` | **`label null7`** —— 形参停在 `Null` 继续跑 ❌ |
| `call_arity_sealed_skew` | 同上，`sealed` 类 ⇒ 调用点去虚化成直接 `Call` | **`label null7`** ❌ |
| `call_arity_static_skew` | v2 `Tag()` / v1 `Tag(string)`，静态方法 | 已抛 `MissingSymbolException` ✅（见下） |
| `call_arity_present` | 不 skew 的对照组 | 照常输出 ✅ |

### 洞的边界（读 `MemberCollector._fillClass` 坐实，不是推断）

- **常规静态方法**：键恒为**全签名 mangle**（`OverloadResolver.MangleKey`）⇒ 签名一变键就变 ⇒ 解析失败 ⇒
  既有的 `undefined function` 已抛。**不在本 change 范围**，`call_arity_static_skew` 留作回归守卫。
- **实例方法 / 静态虚成员**：走「primary 裸键」（声明序第一个同名成员用裸名）⇒ 签名变了**键不变** ⇒ 解析
  **成功**、命中另一个签名。`exec_function` 按 `Frame::new(args, max_reg)` 建帧、**不校验实参数** ⇒ 静默错误答案。
  与构造器站点 ⑤ 同形。

## 实测判据：全量测试上的实参数普查（决定性数据）

在 interp 的三个函数体入口（`exec_function` / `exec_function_from_regs` / `exec_function_from_receiver_regs`）挂探针，
跑完整 `xtask test`，记录所有「物理实参数 ≠ `param_count`」的调用：

- **合法的「实参数 < 形参数」：0 次。** 只有两条 skew fixture（`Label phys=1 pc=2`）。
  ⇒ 默认参数确实由调用点在编译期填满（#623 补齐的正是这一点）⇒ **下界应取 `param_count`，不是 `min_arg`**。
- **合法的「实参数 = 形参数 + 1」：10 个站点**，全是返回 blob 值 struct 的函数（`Pairs.Sum2` / `MakePoint` / `mkPair` /
  `Vec2.op_Add` / `Box.GetPt` / `Range.GetEnumerator` …）。原因是 **sret 约定**：caller 在末尾传一个隐藏的返回槽，
  `FunctionEmitter` 明写它**不计入 SIGS 的 `param_count`**（为了不污染反射/跨包签名）。
- `params` 变长：普查中 0 次出现不相等（调用点已打包成数组）。

⇒ **朴素的「严格相等」会把所有返回 struct 的调用打死。** 而运行时 `Function` 上**没有 sret 标记**。这是本 DRAFT
唯一需要裁决的取舍，见下。

## What Changes —— 落点：只在「首次绑定」处校验，热路径零开销

读代码确认的缓存结构决定了落点：

| 路径 | 首次绑定点 | 之后 |
|---|---|---|
| 合并模块内 `Call`（含急切合并的 `z42.core`） | resolver Pass 2 预填 `method_tokens` | token 直取 |
| 跨包 `Call`（interp） | 填 `cross_module_targets` 的 `OnceLock` | 借用 cell |
| 跨包 `Call`（jit） | tier 3 按名解析后写回 `call_jit_ic` | IC 直取 |
| `VCall`（两后端共用 `resolve_vcall`） | PIC 安装（模块内）/ 每次（跨包 Lazy，本就是慢路径） | PIC 直取 |

- **resolver Pass 2**：签名容不下实参 ⇒ **不预填**（留 `UNRESOLVED`）。JIT tier 1 读的就是 `method_tokens`
  ⇒ 两个后端都自动退到冷路径。**这一点必须做**：`z42.core` 被急切合并进主模块，只查跨包分支会漏掉**对
  stdlib 的 skew**——那恰是用户最常见的形态。
- 冷路径各绑定点（`call()` 未命中写回、cross_cell 填充、jit tier 3、`resolve_vcall` 的各返回点）统一调一个
  `symres` 判据，定案不匹配 ⇒ 抛 `MissingSymbolException`（复用站点 ⑤ 的理由：新异常类要走两-nightly）。
- **判据收敛在一处**（与 `ctor_missing_is_definite` 同款的纯函数，可穷举单测），两后端共用。
- **不做**：常规静态方法（已被 mangle 键挡住）、`CallIndirect`（委托 / 闭包，普查未覆盖到，另量）。

## 判据：SIGS 显式记录 sret（User 2026-09-14 裁决「最根本最终」的方案 C）

**根因**：sret 让被调函数的**物理签名**比 SIGS `param_count` 多一个隐藏形参，但这件事从没写进元数据
（`FunctionEmitter` 为了不污染反射/跨包签名，故意不计入）。运行时拿到的 `param_count` 因此是一个「少报了一」的数，
任何判据都只能猜。

**方案**：`method_flags` 新增 **bit3 = `METHOD_FLAG_SRET`**（bit0–2 = virtual / abstract / sealed，bit3 空闲），
由 `FunctionEmitter` 在 `RetIsStruct` 时置位。运行时判据变成精确的：

```
phys_expected = param_count + (is_instance ? 1 : 0) + (sret ? 1 : 0)
接受 ⇔ phys == phys_expected                       （params 变长：phys ≥ 定长部分，另判）
```

**为什么这是最终方案，另两个不是**：

| 被否方案 | 否决理由 |
|---|---|
| A. 容一（`pc ≤ phys ≤ pc+1`） | 承认猜不准、放宽一格 ⇒「被调方恰好少一个参数」**永远**抓不到，是设计出来的洞 |
| B. 运行时认 blob struct | 在 VM 里复刻编译器 `_isBlobStruct` ⇒ 同一规则两份实现，泛型 / ValueTuple / 布局规则一变即漂移，漂移方向是**误杀合法调用** |
| （另）从 IR 形状推断：非 void 返回但所有 `Ret` 不带值 ⇒ sret | 从函数体反推调用约定，隐含假设更多（例：只抛异常、没有 `Ret` 的函数会被误认成 sret） |

**格式 bump（zbc 1.39→1.40 / zpkg 0.44→0.45）是必须的**：给既有字段加一位语义，若不 bump，旧产物该位恒 0，
新 VM 会把全部「返回 struct 的旧调用」判成错签名。strict-pin 让新 VM 直接拒绝旧产物，两代自举全量重生。
bump 路径已由 #631 修通、#629 实走过。

## 构造器站点 ⑤ 一并收紧（User 2026-09-14 裁决）

`ctor_arity` 用 `min_arg` 当下界，而普查显示合法调用的实参数从不少于 `param_count`（默认值由调用点填满；
ctor 不走 sret）⇒「构造器加了一个可选参数」的 skew 此前会被放过。改为与普通调用**共用同一个判据函数**
（ctor = 实例、无 sret ⇒ 严格相等；`params` 变长同规则）。`ctor_arity` 里那段「`min_arg` 两种口径并存、必须夹住」
的补丁因此整体删除——下界不再读 `min_arg`。

## 途中发现：判定上线即抓出一处真 bug + 一个编译器诊断缺口

第一次跑完整 `xtask test` 时，manifest-target 阶段在 **z42b 内部**抛了本 change 的异常：

```
`Z42Builder._pubBundleProjectDeps` resolved to a definition whose signature does not match this call
(it takes 4 physical argument(s), the call passes 3)
  at Z42Builder._buildProject (builder_commands.z42:69)
```

- **不是误杀**：`_pubBundleProjectDeps(string tomlPath, TomlValue toml, string payloadDir, bool noBuild)`，
  `noBuild` **无默认值**；`builder_commands.z42:69` 只传了 3 个。同包调用，不存在版本 skew——是**源码写错**，
  运行期 `noBuild` 一直静默为 `Null`（当 false 用）。修正为显式传 `false`，行为逐字不变。
- **普查为什么没抓到**：探针挂在解释器的三个函数体入口；z42b 这段走 JIT native 直调，不经过它们。
  判定本身挂在**绑定点**（两后端共用），所以没漏。普查结论（下界 = `param_count`、sret +1）仍成立——
  它证明的是「合法调用长什么样」，这一处恰恰是不合法的。
- **根因是编译器缺诊断**：实测「实参少于必填形参」在**同文件**自由函数、跨文件自由函数、静态方法、实例方法上
  **全部编译通过**（构造器有 E0426，普通调用没有）。z42c 直接发出一条参数不足的 `Call`。
  **不在本 change 修**（重载绑定层的独立 bug），另行登记；本 change 的运行期判定是它在运行期的兜底。

### 追加：CI 上 xtask 自身跑新 VM 后又抓出两处（本地验证盲区）

本地 `./xtask` 启动器默认跑在 nightly 旧 VM 上 ⇒ **xtask 脚本和 z42c 单测从未在新判定下执行过**；CI 用新 VM 跑它们。
补救：本地设 `Z42_PORTABLE_VM=<cargo 新 VM>` 复刻 CI 条件，把同类问题一次清掉。新增两处，均为**真 bug**：

| 位置 | 形态 | 根因 | 修法 |
|---|---|---|---|
| xtask `_driverZpkg` | `common/xtask_layout.z42`（2 参）与 `build/xtask_toolchain.z42`（1 参）**跨文件同名自由函数** ⇒ 同键、运行期只剩 1 参版，所有 `(root, profile)` 调用静默丢掉 profile | 自由函数不重载，但重复定义检查 `_checkDuplicateFreeFunctions` **只查单文件** | 删 1 参版，13 处调用显式传 `"release"`（全部调用方 profile 实为 release ⇒ 行为逐字不变）；同类语义等价的 `_padRight` 重复一并删 |
| z42c 单测 `constraint_member_tests` ×2 | `Assert.True(cond, msg)`，而 `Std.Assert.True` 只有 1 参 ⇒ 实参**过多**、消息静默丢失 | z42c 对普通调用实参个数**两个方向都不查** | 改用 `Assert.Contains(expected, body)`（失败时两值都报） |

全仓静态扫描「同包跨文件同名自由函数」：非测试源码只有 xtask 这两处（`tests/` 与 `examples/` 下为一文件一单元，非同包，属扫描假阳性）。

**待登记的编译器缺口（不在本 change）**：① 普通调用实参个数不校验（过少/过多均放行；构造器有 E0426）；
② 自由函数重复定义只查单文件。本 change 的运行期判定是这两者在运行期的兜底，已拦下 3 处历史静默 bug。
