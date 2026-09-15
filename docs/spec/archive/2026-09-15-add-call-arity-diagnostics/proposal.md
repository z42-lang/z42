# Proposal: 普通调用的实参个数诊断

> **状态：🟢 已实施（User 2026-09-15 确认 DRAFT）** | 创建：2026-09-15
> 同批 DRAFT 的 B 部分（本包自由函数按命名空间解析）拆为独立 change `resolve-free-functions-by-namespace`（PR-2）。
> 来源：`fix-call-arity-skew`（#652）登记的编译器缺口 ①②。那次运行期签名判定上线即抓出 4 处长期静默的真 bug，
> 其中 3 处（z42b 少传 / z42c 单测多传 / 18 处 `File.Copy` 多传）根因是 ①，1 处（xtask `_driverZpkg`）根因是 ②。

## Why

运行期判定只在代码**真正被执行到**时才拦（`File.Copy` 那处只在打包流程里触发，本地测试完全没覆盖）；
编译期诊断能一次静态查出所有调用点。今天 z42c 在这两件事上都不报错。

### A. 普通调用的实参个数不校验

读码 + 普查坐实，口子如下：

| # | 位置 | 现状 |
|---|------|------|
| A1 | `OverloadBinder._resolveOverload` 兜底 | 「同名方法只有一个就不管实参个数」直接选中。本意只服务 `Int32.ToString(n)`（静态形式调实例方法、接收者作首参） |
| A2 | `_withDefaults` 同包少传 | `_adaptArgs` 补不齐（缺无默认值形参）返回 null → **原样返回不足的实参** |
| A3 | `_crossPkgDefault` 跨包少传 | 无 `$Default` 的形参**补零值** ⇒ 实参个数「对了」，**连 #652 运行期判定都拦不住** |
| A4 | 四条旁路只做 `BindArgsToSignature` | 同类无限定静态调用 / 局部函数 / `ns.func()` 形式 / 静态形式调实例方法（实参数 ≠ 形参数+1 时静默发码） |

对照：构造器早有 **E0426**；`E1005 MissingRequiredArgument` **码已定义、全仓零发码点**。

## 普查（全仓：compiler + stdlib + scripts + toolchain + `xtask test` 全部 fixture，GREEN 状态下挂探针）

| 探针 | 命中 | 结论 |
|------|------|------|
| A1 兜底 | 214 | 161 = `Int32/Int64.ToString(n)` 接收者作首参（合法）；53 = 少传但**有默认值**（`Log.line()` caller 宏等，`_withDefaults` 正确补齐，合法） |
| 最终实参数 ≠ 形参数 | 162 | 161 = 上面的接收者作首参；**1 = `examples/exceptions.z42:43` `int.TryParse(s, out var n)`**（C#-ism；该文件已在 `examples-known-broken.txt`，原因栏补一条即可） |
| A2 同包少传 / A3 跨包补零 | **0 / 0** | #652 已修掉全部既有调用点 |
| A4 旁路（无限定静态 / 局部函数 / `ns.func()`） | **0** | — |
| 静态形式调实例方法 | 161 | 全部是 `实参数 == 形参数+1` 的接收者作首参，**无违例** |
| B 同名重复注册 | 19 | 同文件（已有 E0408 覆盖；含 known-broken `patterns.z42` 的解析残渣、REPL 单测）；**跨 ns 合法同名**：multi-exe 用例的多个 `Main`（`two_mains`/`ns_same_short_name`/`CacheMulti`）、z42c.semantics 四个单测文件各自的 `bodyDiags`/`countCode`/`pre`（**签名碰巧相同才没炸**）；**同 ns 跨文件：0** |

⇒ **A 落地零既有调用点要改**（只动一个 known-broken 名单的原因栏）。（B 行的普查结论见 `resolve-free-functions-by-namespace`。）

## What Changes

### A 实参个数诊断（`compiler`，纯诊断、零发码变化）

- 判定收敛到 `OverloadBinder` 一处 `_checkCallArity`，所有调用形态（实例 / 类限定静态 / 基元关键字静态 / ns 限定静态 /
  同类无限定静态 / 自由函数 / `ns.func()` / 局部函数）汇到它：
  - 非 params：实参数 > 形参数 → **E1006 TooManyArguments**（新码）
  - 缺位形参既无默认值（同包看 `Decl.Default`；跨包看 `$Default` / caller 宏）→ **E1005 MissingRequiredArgument**（接上既有空码）
  - **静态形式调实例方法**：只接受「实参数 = 形参数 + 1」（接收者作首参），否则同上两码、消息点明「以静态形式调用实例方法时首个实参是接收者」
- A1 兜底**保留「唯一候选即选中」**，只是不再跳过校验——这样报的是精确的「期望 N 个、给了 M 个」而非笼统的「no method」
- A3 删除补零：跨包无默认值即 E1005
- 消息形状与 E0426 一致；**语义层用字面量发码**（同 E0449–E0468 手法，避 core→semantics 新跨成员符号 F2 冷启动）
- `examples-known-broken.txt` 的 `exceptions.z42` 原因栏补 `int.TryParse(s, out var n)`
- 负例 fixture：每形态阳性 + 在场对照；退回对照（撤掉判定 → fixture 全 FAIL）

### 非目标（已登记在别处，不在本 change）

- 重载决议按精确 arity 过滤、不考虑默认值形参（`ctor-silent-bugs` 程序剩余项）
- 「静态形式调实例方法」这一写法本身是否保留（非 C# 语法、161 处在用）——本 change 只收紧、不删

## 格式 / 种子影响

- 无 zbc/zpkg 格式变更，无新语法 ⇒ 不需要两-nightly
- 不改发码 ⇒ 不需 CompilerFingerprint +1（实测：冷构建 stdlib 22 库对 base 逐字节对账，只有源码本身改了的 `z42c.core` 不同）
