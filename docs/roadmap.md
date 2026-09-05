# z42 Roadmap

> **本文档 = z42 唯一的迭代计划**：当前焦点、下一阶段、长期 SemVer 路线、未完成项索引。
>
> **已完成**：每个落地的功能对应一个 `docs/spec/archive/YYYY-MM-DD-<name>/` 归档目录（带完整 proposal / design / tasks / 实施备注）；本文不复述。需要查"X 何时落地、为什么这样设计"按主题或日期检索 [`docs/spec/archive/`](spec/archive/) 即可。
>
> **设计决策**：见 [`docs/features.md`](features.md)（决策 + 理由 + phase 归属）+ [`docs/design/philosophy.md`](design/philosophy.md)（顶层哲学）。
>
> **实施细节**：见 [`docs/design/`](design/) 5 个主题子目录。

---

## 设计目标

z42 是一门**全栈系统编程语言**：从嵌入式固件到云端后端，无需切换语言。融合 C# 语法 + Rust 纪律 + Python 易用性。

| 维度 | 设计 |
|------|------|
| 语法 | C#（命名 / 声明 / OOP 结构）|
| 内存 | 始终 GC（无所有权 / 借用 / 生命周期）|
| 错误处理 | L1 异常 / L3 引入 `Result<T, E>` + `?`（共存）|
| 类型系统 | 静态类型 + 局部推断 + 泛型 + 接口；L3 引入 Trait 静态分发 |
| 执行模型 | Bytecode-native：Interp / JIT / AOT 三模式，命名空间级 `[ExecMode]` 注解切换 |
| 嵌入 | VM 设计为可嵌入到外部 app（C ABI），目标 ~200KB 子集 |
| 互操作 | 三层 ABI（C / Rust ergonomic / 平台 facade），native 类型可注册进 z42 |

性能基线（philosophy §9）：interp ≤ Python 1.5×；JIT ≥ V8 70%；AOT ≥ Go 80%；GC pause < 5ms p99；嵌入子集 < 200KB。

---

## 固定决策

- **GC**：z42 始终带 GC，永不引入所有权 / 借用（降低上手成本）
- **IR**：寄存器 SSA 形式
- **执行模式注解**：作用于命名空间级
- **`.zbc` magic**：`ZBC\0`
- **pre-1.0 不承诺向后兼容**（与 [`philosophy.md` "不为旧版本提供兼容"](../.claude/rules/philosophy.md#不为旧版本提供兼容2026-04-26-强化) 对齐）
- **1.0+ 启用 SemVer + deprecation 周期**

---

## 阶段总览

| 阶段 | 目标 | 状态 |
|------|------|:----:|
| **L1** | C# 基础子集，跑通完整 pipeline（源码 → IR → VM 执行） | ✅ 已完成 |
| **L2** | 基础设施（编译、工程、测试、VM 质量、标准库） | 🚧 进行中 |
| **L3** | 高级语法（泛型 / Lambda / async + z42 特有特性） | 🟡 部分（泛型 + lambda + delegates 提前落地）|

阶段串行：L1 全通 → L2；L2 全完成 → L3。当前 L1 全绿、L2 多项进行中、L3 部分提前落地。

---

## 当前焦点

**0.3.x 自举线**：以 GC v1 为地基，三主线并行——A（stdlib 重组 + perf）‖ B（**编译器全自举**：7 子系统 byte-identical）‖ C（**反射完整化**：含非泛型 Method.Invoke），尾段五项扩展——boxing 机制 + test-runner 删除 + CI 三平台模拟器 + workload B1/B2，REPL 作为 capstone。详见 [`plan-0.3.x-three-streams/proposal.md`](spec/archive/2026-06-19-plan-0.3.x-three-streams/proposal.md)（2026-06-19 扩展）。

> **2026-06-07 重排要点**：全端到端自举从 1.0 拉到 0.3.x 作为本线招牌；自举采用「受限写法 + dogfood 补真卡点」（不为自举强制提前整个 0.6/0.7 的 match/LINQ/Result）；REPL 从 0.5.x 拉到 0.3.x capstone。连锁影响见下文 [长期 SemVer 路线](#长期-semver-路线05--10) 重排。

> **2026-06-19 扩展要点**：boxing 机制（0.3.11）作为非泛型 Method.Invoke 前置新增；Method.Invoke 非泛型从 0.5.x 拉入 0.3.12；IsEnum + 嵌套泛型反射 + 接口成员枚举并入 0.3.12；test-runner 删除（z42b test GA）新增 0.3.13；CI 三平台模拟器（WASM/iOS Simulator/Android Emulator → GitHub Checks）新增 0.3.13 并行；workload B1+B2（命令发现 + 包格式）新增 0.3.14；REPL 后移 0.3.15；soak 0.3.16。0.5.x 反射收窄为泛型方法 Invoke + MakeGenericType + Activator.CreateInstance<T>。

### 0.2.x — 工程化 & 包系统 + perf CI ✅ 收尾

退出标准（已达成）：`.zbc` v1.x / `.zpkg` 格式冻结 ✅；perf CI 上线 ✅；多平台 CI matrix 全绿 ✅；release 自动化产出跨平台 binary ✅。

| 子版本 | 内容 | 估时 |
|------|------|:----:|
| 0.2.0 | `.zbc` v1.x 格式冻结（strict-pin + 6 fixture 字节 golden + workflow.md bump 流程）— [archive/2026-05-14-freeze-zbc-v1](spec/archive/2026-05-14-freeze-zbc-v1/) | 1 周 |
| 0.2.1 | `.zpkg` indexed/packed 格式冻结（strict-pin + 4 fixture 字节 golden + 0.5 → 0.6 catch-up bump）— [archive/2026-05-14-freeze-zpkg-v0](spec/archive/2026-05-14-freeze-zpkg-v0/)；`z42c disasm` 完整化作为另一半（视实施 — follow-up spec）| 1 周 |
| 0.2.2 | Benchmark 套件骨架（`cargo bench` + BenchmarkDotNet）+ 初始基线 | 1.5 周 |
| 0.2.3 | ✅ Perf CI + 性能预算 (`.github/workflows/bench-pr.yml`, 2026-06-05) — PR-side workflow fetches baseline from `bench-baselines` branch, runs `xtask bench --diff --threshold-time 0.10`, fails on >10% time regression | 1 周 |
| 0.2.4 | 🟡 部分 — ✅ `lint-manifest` WS008/WS009 (2026-06-04 `2c5a1881`); ❌ `z42c new/init/fmt/clean` + 独立 `z42-fmt` binary 推 0.4.x | 1 周 |
| 0.2.5 | ✅ 多平台 CI matrix（[ci.yml](../.github/workflows/ci.yml) 5 平台 build/test）+ CI 模板 | 1.5 周 |
| 0.2.6 | ✅ Release 自动化（[release.yml](../.github/workflows/release.yml) tag → 跨平台 binary + zpkg；[archive/2026-05-14-add-release-automation](spec/archive/2026-05-14-add-release-automation/)）| 1 周 |

### 0.3.x — 自举线（GC v1 地基 → stdlib ‖ 全自举 ‖ 反射 → REPL）（2026-06-07 重排）

退出标准：（A）stdlib 重组完成 + 每包 bench baseline + 三轮 perf 攻坚；（B）**编译器 7 子系统全部用 z42 重写**，byte-identical CI gate 7 日零飘移 + end-to-end compile-perf median ≤ 3× C#；（C）**反射完整化**（非泛型 Method.Invoke + IsEnum + 嵌套泛型 GetGenericArguments + 接口成员枚举）；（D）**test-runner 删除**（`z42b test` 端到端 GA）；（E）**CI 三平台模拟器**（WASM / iOS Simulator / Android Emulator → JUnit → GitHub Checks 全绿）；（F）**workload B1+B2**（命令发现 + 包格式 + `z42 workload install/list/remove` GA）；（capstone）z42 原生 REPL。

> **完整规划**见 [`plan-0.3.x-three-streams/proposal.md`](spec/archive/2026-06-19-plan-0.3.x-three-streams/proposal.md)（2026-06-07 重排，supersede 2026-06-05 保守版）。以下为子版本索引。
>
> **B 主线＝本版本招牌（全自举，从原 1.0 拉到 0.3.x）**：7 子系统 = `z42.{Core,Syntax,Project,Driver,Semantics,IR,Pipeline}` 1:1 镜像 C# 项目，源码落 `src/compiler/` 独立顶级目录（与 `src/compiler/` 平级；2026-06-07 User 裁决，覆盖原 `src/z42.compiler/`；子目录名==包名 `z42c.<sub>`，产物 `z42c.<sub>.zpkg`）。**受限写法**：class+虚方法替代 record+match / 循环替代 LINQ / 异常替代 Result；只有自举真卡点才 dogfood 在 z42 里补该特性（禁止 workaround，per `feedback_dogfood_fill_gaps`）。**无桥接**：z42 端只 ship 就绪命令（0.3.4 起 lex/parse/manifest-check、0.3.9 起 build），0.3.x default 编译器仍是 C#，两实现并存逐字节对账。
>
> **受限写法 ⇒ 不强制提前半个 L3**：match/ADT/LINQ/Result 完整版仍在 0.6/0.7；只有被自举单点阻断的特性才按 features.md 逐项评估提前。这是「受限写法」决策的直接后果。
>
> **REPL = capstone（从原 0.5.x 拉到 0.3.x）**：自举端到端 build 跑通后落地（前置 Semantic/TypeChecker/IR 均在本线内交付），单独 spec `add-z42-repl`。
>
> **C 主线完整化含非泛型 Method.Invoke（2026-06-19 调整）**：boxing 机制（0.3.11）为前置；非泛型方法 Invoke 在 0.3.12 落地。泛型方法 Invoke + MakeGenericType + Activator.CreateInstance<T> 仍依赖 generic instantiation，推 0.5.x L3-R。
>
> **C3 Attribute reflection 前置**：用户自定义 attribute 机制 spec 需先落地（0.3.4 起草）。

**0.3.0（地基）**：GC v1 —— 抽象 GC 接口 + mark-and-sweep（替换 `Rc<RefCell>`），A/B/C 共同前置（估 2–3 周）。

| 子版本 | B 自举（招牌）| A stdlib | C 反射 |
|:--:|------|------|------|
| 0.3.1 | B0 架构 spec + 建 `src/compiler/` 7 子包骨架 + xtask `build/test compiler`（[scaffold-z42c-selfhost](spec/changes/scaffold-z42c-selfhost/)）| A0 包审计 spec | C0 反射 API spec（新建 `reflection.md`）|
| 0.3.2 | — | A1 包重组（先行，稳定 B 引用路径）| — |
| 0.3.3 | core + syntax（Lexer/Parser/AST）+ bit-identical gate | A2 bench baseline | C1 metadata 暴露 + 4 反射对象 + `GetMembers` 系列 |
| 0.3.4 | project + driver（lex/parse/manifest-check 可跑）| A3 perf #1 BigInt/Coll | C2 `typeof(T)` + `obj.GetType()` + z42.reflection 包公开 |
| 0.3.5 | **semantics**（首个硬子系统，dogfood 高发段）| A4 perf #2 String/IO | C3 Attribute（前置 attribute 机制 spec）|
| 0.3.6 | typecheck | A5 perf #3 JSON/YAML/TOML | — |
| 0.3.7 | ir（codegen + lowering，寄存器 SSA）| | |
| 0.3.8 | emit（ZbcWriter/ZpkgWriter → byte-identical .zbc/.zpkg）| | |
| 0.3.9 | ✅ 归档 port-z42c-self-compile（G22 全绿）+ runtime-dynamic-load-call 归档 ‖ **✅ z42c 编译全 22 stdlib 包 byte-identical**（z42c 可完整替代 C# 编译器；commit 36485ae4，2026-06-19）| | |
| 0.3.10 | byte-identical CI gate 全 7 子系统 7 日零飘移 + compile-perf gate（median ≤3× / P99 ≤5×）启用 | | |
| 0.3.11 | **Boxing 机制**（方案 A 类型系统层）：prim→object 隐式装箱（Value 已是 tagged union → codegen no-op，零分配）+ object→prim 受检拆箱（复用 Convert）。实证：编译器侧（GS6 赋值）已存在，整改动 = 运行期 Bool 拆箱恒等 + golden + 文档。**无 box/unbox IR、无格式 bump**。见 [`design/language/boxing.md`](design/language/boxing.md) | 🟢 实现完成（add-boxing-conversions，待归档）| |
| 0.3.12 | **反射完整化**：~~Method.Invoke（非泛型）~~ ✅ + ~~Type.GetType(fqn)~~ ✅（add-method-invoke-non-generic；builtin 复用 exec_function，异常原类型传播 interp+jit；Activator 无参延后）‖ ~~IsEnum~~ ✅（2026-07-09）‖ ~~嵌套泛型 GetGenericArguments~~ ✅（2026-07-23 add-reflection-nested-generic-args，方案 A：z42c 发括号实参串 + runtime 递归解析，无格式 bump / TypeofInstr 接口不变）‖ ~~接口成员枚举~~ ✅（2026-07-20 add-interface-member-reflection，纯 runtime surface zbc 1.28 接口方法块）| ✅ 反射完整化收口（非泛型 Invoke/GetType/IsEnum/嵌套泛型 args/接口成员枚举 全落地；泛型 Invoke/MakeGenericType/Activator<T> 属 0.4.x G） | |
| 0.3.13 | **test-runner 删除**：z42.test 加 TestRunner/BenchRunner（反射驱动 [Test]/[Benchmark] 发现）+ z42b `test`/`bench` verb + 退役 Rust binary（同替两者）‖ **CI 三平台模拟器**：WASM(Playwright) / iOS Simulator(`xcodebuild -destination`) / Android(emulator-runner+KVM) → JUnit → GitHub Checks（stdlib ‖ toolchain 双锁并行）| | |
| 0.3.14 | **workload B1**（命令发现：launcher 扫目录 → Std.Cli 树合并）+ **B2**（workload 包格式 + `z42 workload install/list/remove`）| | |
| ~~0.3.15~~ | ~~**REPL capstone**~~ → **上移 0.4.0**（2026-07-15，与 Playground 同批作产品能力；见 0.4.x 段模块整合表）| | |
| 0.3.16 | 收尾：z42c-selfhost 下全 dotnet/xtask test 绿 + soak + A perf delta report | | |

**‖ = 三主线在该子版本并行推进**。子版本号弹性——本线终点由退出标准定义，自举 dogfood 补特性时插入特性 spec 子版本。

**重排沿革**：
- **2026-06-19（五项扩展）**：boxing 机制（0.3.11）新增；Method.Invoke 非泛型从原 0.5.x 拉入 0.3.12；IsEnum + 嵌套泛型 GetGenericArguments + 接口成员枚举并入 0.3.12；test-runner 删除新增 0.3.13；CI 三平台模拟器（WASM/iOS Simulator/Android Emulator）新增 0.3.13 并行；workload B1+B2 新增 0.3.14；REPL 后移 0.3.15；soak 后移 0.3.16。0.5.x 反射收窄为泛型方法 Invoke + MakeGenericType + Activator.CreateInstance<T>。
- **2026-06-07（全自举）**：原"B 只做 4 子系统（Lexer/Project/Driver/Parser）+ 剩余推 0.5.x"→ 全 7 子系统并入本线；原"REPL 推 0.5.x"→ 本线 capstone；原"byte-identical 推 1.0"→ 本线退出标准；原"compile-perf gate 0.5.x 启用"→ 0.3.10 启用。
- **2026-06-05（从 0.3.x 移出，仍生效）**：Golden 全 L1 覆盖 + interp/JIT 一致性 CI / 调试符号 / Profiler hooks → 0.4.x 起；热重载 VM 完整实现 → 0.5.x 起；GC v1 → 0.3.0（提前）。

### 0.4.x — 质量与性能线（4 流并行 + G 前置流）（2026-06-23 重定位；2026-07-15 按模块整合 todo-list）

> **模块视图（2026-07-15，权威范围）**见 [`replan-0.4.0-by-module/design.md`](spec/changes/replan-0.4.0-by-module/design.md)：以 `docs/todo-list.md` 第 11 行为 0.4.0 权威范围，把下方 four-streams 的 P/B/S/L/G 作为实现细节回填进 6 模块（编译器 / 语法机制 / 标准库 / runtime / 工具链 / 测试·产品·文档）。整合新增/上移项：**REPL 从 0.3.15 上移** + **Playground** + **runtime 组件化 + host/hostrun/main 统一（从 0.9.5 上移）** + **z42c 基础库(metadata/ir)入 stdlib（沿用 `converge-z42c-onto-z42-project` 收敛范式）** + **tier2 平台测试补齐（wasm/ios/android）** + **book 整理**。
>
> **完整流规划**见 [`plan-0.4.x-four-streams/`](spec/archive/2026-06-23-plan-0.4.x-four-streams/)（已归档）。原线性"填 stdlib 包"框架（0.4.0 core → … → 0.4.8 docgen）作废——24 个 stdlib 包已 ship，0.4.x 真实价值是**兑现性能杠杆 + bench 工具 GA + 补齐小语法 + 打磨已有 stdlib + 产品能力（REPL/Playground）+ 工具链 GA**，而非建包。沿用 0.3.x 子系统互斥锁的多主线并行模型。

退出标准：（P）P1 JIT 算术拆箱 + P2 inline cache 落地且 bench 证明收益 + 触及库 baseline 化；（B）独立 `z42.bench` 包 + `z42b bench` GA + e2e 硬门禁 + PR 自动 diff 评论；（S）`params`/`init`+表达式体属性/索引器/命名实参/`partial` 全部 GREEN + dogfood 验证；（L）JSON `Deserialize<T>` 泛型 serde + CLI 校验/全局flag/补全 + 模块审计清零 + `z42-doc` 无错 + z42c 基础库(metadata/ir)入 stdlib；（G）泛型实例化 + 泛型反射三件套（Invoke/MakeGenericType/CreateInstance<T>）落地；（R8）runtime host/hostrun/main 统一 + 组件化 cargo-feature 骨架；（X）REPL + Playground 可用 + tier2 平台（wasm/ios/android）测试流程绿 + book 整理。

| 子版本 | P（perf：Pv VM 侧 ‖ Pc 编译器侧）| B（bench）| S（syntax）| L（lib）| G（泛型前置）|
|:--:|------|------|------|------|------|
| 0.4.0 | Pv0 perf 基线刻画（量化已落地的 4-slot IC / JIT I64 特化）| B1 `z42.bench` 包 + B2 `z42b bench` GA | `params` 变长参数 ✅（add-params-varargs，2026-07-01）| L1 stdlib 模块审计 spec | G0 泛型实例化设计 spec |
| 0.4.1 | Pv1 quickening + 超指令 ‖ Pc1 激活 IrPassManager（const-fold/DCE）| B5 perf 库 baseline 铺面 | `init` + 表达式体属性 | L2 `JsonReader`（合 add-json-streaming-reader）| G1 运行期泛型实例化 |
| 0.4.2 | **Pv2 JIT 直接 emit 拆箱 + F64 特化（招牌）** ‖ Pc2 intrinsic 表 + devirt pass | B3 e2e 硬门禁 | 索引器 `this[i]` | L2 `JsonSerializer` 非泛型（`[JsonProperty]`）| G2 泛型方法 Invoke + `MakeGenericType` |
| 0.4.3 | Pv3 Frame 寄存器 HashMap→Vec ‖ Pc3 大类拆分 + BindCall D-11 收尾 | B4 PR 自动评论 diff | 命名实参 | **L2 `Deserialize<T>` 泛型 serde（招牌）✅ add-json-serde（M2；无格式 bump——属性 attr 复用 field_attributes 挂 `__prop_X` 背后字段）** | **G3 `Activator.CreateInstance<T>` ✅ add-generic-activator（泛型薄壳 + 方法级形参转发 `$mta:<idx>`；无格式 bump）** |
| 0.4.4 | Pv4 非原子 refcount（profiling 门控）‖ Pc4 compile-perf phase profiling | — | `partial` class | L3 CLI 值校验 + 全局 flag | — |
| 0.4.5 | P6 stdlib 脚本 perf 三轮（BigInt/Coll、String/IO、JSON/YAML/TOML）| bench 收尾报告 | — | L3 CLI shell 补全 + L1 审计清单执行 | — |
| 0.4.6 | — | — | — | **`z42-doc` 文档生成器**（doc comment → HTML/markdown + stdlib 自动发布）| — |

**‖ = 五流（含 G）在子版本并行**；子版本号弹性，由退出标准定义终点，按子系统锁可用性排队。

> **P 流分两侧并行**：**Pv（VM 侧）**吃 `runtime` 锁——JIT 拆箱 / quickening / Frame 表示 / 非原子 refcount，与编译器侧并行；**Pc（编译器侧）**吃 `compiler`+`z42c` 锁——IrPassManager 首批 pass / intrinsic 表 / devirt / 大类拆分，**任何改 codegen 的 pass 必须 C# + z42c 双侧镜像**（否则破坏 0.3.10 byte-identical gate），故 Pc 与 S/G 串行争锁。**已落地基线**（不重复做）：4-slot 多态 IC（FieldIC/VCallIC，2026-05-28）、JIT I64 helper 特化（2026-05-28）、cross-zpkg OnceLock 缓存（2026-06-11）、Instruction enum 96B→32B（2026-06-11）、GC v1 三阶段（2026-05-22）。两侧框架与 perf 杠杆全表见 [`plan-0.4.x-four-streams/design.md`](spec/archive/2026-06-23-plan-0.4.x-four-streams/design.md#p-流细化编译器侧--vm-侧框架与性能)。

**G 流连锁（2026-06-23 User 裁决"硬上完整泛型 serde"）**：`Deserialize<T>` 自动绑定任意类型依赖运行期泛型实例化 + 泛型反射，原排 0.5.x → 提前到 0.4.x G 流作为 L 流招牌前置。代价：违反"不为单点提前半个 L3"，作显式例外登记；0.5.x 反射条目相应清空。缓解：JSON 两步交付（先非泛型 `JsonSerializer` 保产物，G 就绪再上泛型版）。

**锁协调**：`stdlib` 锁被 L 流 + P6 + B5 三处争用 → 串行排队；`compiler`/`z42c` 被 S 流 + G 流同时吃 → 串行/合并节奏（详见 proposal Open Questions）。

**移除项**（被本线提前，从他处删）：原 0.4.7「z42.bench」并入 B 流 0.4.0；原 0.5.x「反射泛型扩展」上移 G 流。

**模块整合新增项（2026-07-15，todo-list 第 11 行；four-streams 表未含，作 R/X/M 列补入）**：

| 模块 | 项 | 来源 / 现状 |
|------|----|------|
| runtime | **R8a host/hostrun/main 统一**（不同平台共享简化）+ **R8b 组件化 cargo-feature 骨架** | 原 0.9.5 上移；R8b 完整裁剪留后续 |
| 工具链 | **z42b GA**（统一前端）+ publish 脱 desktop + workload 命令自动注册 + xtask 路径读 z42.toml + package 剥离调试符号 | in-flight `wire-z42b-host-build` / `add-workload-command-dispatch`；todo#2/#8/#9/#10 |
| 编译器 | **增量 + 并发编译** + build 依赖排序 + 版本 hash 触发重编 | todo#1/#4/#7；并入 Pc5 |
| 标准库 | ~~**z42c 基础库(metadata/ir)入 stdlib**~~ ✅ 2026-07-21 | 已落地：IR + zbc + zpkg 后端合一入 stdlib 单库 `z42.ir`（converge-z42c-ir-metadata-onto-stdlib；User 定单库 + CacheStore 留构建侧）。z42c 现 5 子包，self-host 5/5。（原「后端拆 z42c.zpkg」更正为下沉 z42.ir） |
| 产品 | **REPL**（原 0.3.15 上移）+ **Playground** | in-flight `add-z42-wasm-playground` |
| 测试 | **tier2 平台测试补齐**（wasm/ios/android → GitHub Checks）；当前仅全测 tier1 | `versions.toml [platform.*]` tier 定义 |
| 文档 | **book 整理与内容补充** | docs 不上锁，贯穿 |

---

## 长期 SemVer 路线（0.5 → 1.0）

> 高层 charter；每个 minor 启动时再开 spec 排具体子版本。设计原则：每个 minor 是独立可发布单位（用户可感知能力跃迁）；每个 patch 是独立 spec。

| 版本 | 主题 | Phase | 估时 |
|------|------|:----:|:----:|
| **0.5.x** | **G2/G3：泛型方法 `Invoke`/`MakeGenericMethod` + `Deserialize<T>` serde 串联** + **Trait 静态分发** + **deopt + JIT 分层**（与 hot-reload 共用地基）+ **LSP v1** + **Interop 2a 稳定化**（`z42-abi`/`z42-host` embedding 打磨到稳定）| L3 | 8–12 周 |<br>（**2026-08-21 按代码现状重修订**——已落地、从本版移除：G1 `MakeGenericType`+constructed `CreateInstance` ✅、OSR interp→JIT 热替换 ✅、Interop 2a 地基 `z42-abi` crate+C ABI 头+`z42-host` ✅、JSON 解析/写入 `z42.json` ✅。反射泛型扩展原 2026-06-23 上移 0.4.x G 流，其 G1 已交付、G2/G3 未落地 → 顺延本版。**2026-08-21 add-generic-methods M1**：G2 的**直接调用**部分已交付——方法级 type_args 端到端（`Foo<T>()` + 方法体 `typeof(T)`/`new T()`/`default(T)`，frame 槽载体，zbc 1.36/zpkg 0.41），是 `Deserialize<T>` serde 招牌的语言前置；剩反射式 `MakeGenericMethod().Invoke()` + 类型推断见 Deferred Backlog。见 [`book/src/language/generic-methods.md`](book/src/language/generic-methods.md)。**2026-08-22 add-reflective-invoke G2**：反射式泛型方法 `MakeGenericMethod().Invoke()` + `IsGenericMethod`/`GetGenericArguments` 已交付（复用 M1 帧槽，**无格式 bump**——zbc SIGS 段早预留方法类型形参槽）；同 change 补齐反射层级 `MethodBase`/`ConstructorInfo` + 带参构造 `ConstructorInfo.Invoke(args)`。剩 `Deserialize<T>` serde 引擎（L 流）+ 类型推断见 Deferred。）
| **0.6.x** | 函数式（Lambda / 命名参数 / 模式匹配 / `let` 不可变 / LINQ）+ unmanaged + GC v2 + linter | L3 | 9–11 周 |
| **0.7.x** | `Result<T,E>` + `?` + ADT + `match` 穷尽检查 | L3 | 6–8 周 |
| **0.8.x** | async / await + 多线程 + GC v3（generational + concurrent）+ DAP debugger | L3 | 12–16 周 |
| **0.9.x** | 单文件脚本 + 嵌入 API GA + 可裁剪 + WASM target + Interop 2b（manifest reader / source generator）| L3 | 10–14 周 |
| **0.10.x** | 性能强化（philosophy §9 五指标全部达标）| L3 | 8–12 周 |
| **1.0.x** | 删 C# bootstrap（自举核心已在 **0.3.x** 完成 byte-identical）+ 跨架构 NativeAOT + Interop 3 + `z42up` 工具链 GA + SemVer / deprecation 启用 | L3+ | 8–12 周 |

**累计估算**：~16–20 个月（按全职 1 人节奏）。

### 跨版本关键依赖

```
0.1 ─► 0.2 ─► 0.3 ──┬──► 0.4 ──► 0.5 ──► 0.6 ──► 0.7 ──► 0.8 ──► 0.9 ──► 0.10 ──► 1.0
       │       │   │           │                                            │
       │       │   ├── reflection C1-C3 (0.3 C ✅) ──► boxing 机制 (0.3.11) ──► Method.Invoke 非泛型 (0.3.12) ──► MakeGenericType+constructed CreateInstance (0.4 G1 ✅) ──► 泛型方法 Invoke (0.5 G2) ──► Deserialize<T> serde (0.5 L)
       │       │   │                                                        │
       │       │   ├── 编译器全自举 7 子系统 (0.3 B：Lex→Parse→Proj→Driver→Sem→TC→IR→Emit→Pipeline)
       │       │   │           ──► byte-identical gate + compile-perf ≤3× (0.3.x 退出)
       │       │   │                          ──► 删 C# bootstrap (1.0 收尾)
       │       │   │                                                        │
       │       │   ├── GC v1 (0.3.0，从 0.3.3 提前) ──► GC v2 (0.6) ──► GC v3 (0.8) ─►
       │       │   │                                                        │
       │       │   └── stdlib 重组 + perf (0.3 A) ──► stdlib v1 (0.4)
       │       │                                                            │
       │       └─── benchmark 套件 ─► perf CI (0.2.3) ──持续生效──────────► │
       │                                                                    │
       └─── .zbc/.zpkg 格式冻结 ─────► 1.0 SemVer 启用 ────────────────────► │
```

强依赖链：
- 0.3 A perf 攻坚 ◄── 0.3.0 GC v1（无稳定 GC 的 micro-opt 无意义）
- 0.3 B 编译器全自举 ◄── 0.3.0 GC v1（z42 端编译器对 GC 压力大）
- 0.3 B 自举受限写法 ◄── 泛型 G1-G4 + 闭包核心（已提前落地）；缺 match/LINQ/Result 用 class+虚方法 / 循环 / 异常替代，真卡点才 dogfood 提前
- 0.3 C3 Attribute reflection ◄── 用户自定义 attribute 机制（features.md §X，0.3.5 前先 spec）
- 0.3.11 boxing 机制 ◄── 0.3.12 Method.Invoke 非泛型（auto-boxing prim→Object 是 Invoke 的直接前置）
- 0.4 G 流泛型反射扩展（泛型方法 Invoke + MakeGenericType + Activator.CreateInstance<T>）◄── 0.4 G 流运行期泛型 instantiation（2026-06-23 从 0.5.x 提前，支撑 0.4 L 流 Deserialize<T> serde）
- 0.4 L 流 JSON `Deserialize<T>` 完整泛型 serde ◄── 0.4 G 流泛型实例化 + 泛型反射（User 裁决"硬上"，显式 L3 提前例外）
- 0.5 反射 ◄── 0.10 性能数据自查（type metadata access）
- 0.6 unmanaged ◄── 0.9.6 C ABI 头文件
- 0.7 Result ◄── 0.8 async（async fn 通常返回 `Task<Result<T,E>>`）
- 0.8 GC v3 ◄── VM 组件化（cargo-feature **骨架 + host 统一**已上移 0.4.0 R8，2026-07-15；完整裁剪粒度仍在 0.9.5，Q8 待裁决）
- 0.10 性能基线 ◄── 1.0 稳定承诺
- 1.0 删 C# bootstrap ◄── 0.3.x 自举 byte-identical gate 跑稳（自举核心不再等全部 L3；受限写法已规避 match/LINQ/Result）

---

## Feature → Version 映射

每个 features.md 章节落地到哪个 minor。

| features.md 章节 | 所属 minor | 当前状态 |
|------|:------:|:----:|
| §1 Type System / §2 Null Safety / §3 Memory Management / §4 Error Handling (exceptions) / §5 Type Definitions (class/struct/record) / §6 Functions / §7 Control Flow / §8 Strings / §9 Collections / §10 Imports / §11 Numeric Aliases | 0.1.x | ✅ L1 |
| §12 Hot Reload | 0.5.x（从 0.3.2 推后；GC v1 后真热更新落地）| 🟡 设计有 |
| §13 Execution Mode Annotations | 0.1.x（注解）→ 0.5.x（运行时切换；从 0.3.x 推后）| 🟡 注解 ✅；运行时切换待 |
| §14 Generics + Trait | 0.5.x | ✅ G1-G4 + L3-Impl 提前落地 |
| §15 Reflection | **0.3.x C主线**：只读元数据 + typeof/GetType + Attribute（C1-C3 ✅）；GetInterfaces / IsArray / IsAbstract 等扩展 ✅；**完整化（0.3.12）**：非泛型 Method.Invoke + IsEnum + 嵌套泛型 GetGenericArguments + 接口成员枚举（boxing 机制 0.3.11 为前置）；**0.4.x G 流泛型扩展**（2026-06-23 从 0.5.x 提前）：运行期泛型实例化 + 泛型方法 Invoke + MakeGenericType + Activator.CreateInstance<T>（支撑 0.4 L 流 Deserialize<T> serde）| 🟡 C1-C3 + 多项扩展已落地（见 spec/archive 2026-06-09–06-17 系列）；boxing + Method.Invoke 待 0.3.11–0.3.12；泛型扩展待 0.4.x G 流 |
| §16 Lambda + Closure | 0.6.0 | ✅ L2-C1 + L3-C2 核心提前落地 |
| §17 Result + ADT + match | 0.7.x | 📋 |
| §18 可裁剪 / Tree-shaking / 200KB 子集 | 0.9.x（嵌入 / 裁剪）+ 1.0-rc（AOT 静态链接）| 📋 |
| §19 NativeAOT | 1.0.x | 📋 |
| §20 Interop 三层 ABI | 0.5.5 / 0.9.x / 1.0.x | ✅ Tier 1 + Tier 2 + manifest 提前落地 |

> "提前落地" = L2 阶段已实施部分 L3 特性，未对应到 0.x.0 minor 但代码已在 main。

---

## 横向工作流（贯穿所有版本）

| 工作流 | 启用版本 | 内容 |
|------|:------:|------|
| Benchmark 套件 | 0.2.2 | `cargo bench` + BenchmarkDotNet 骨架 |
| Perf CI | 0.2.3 | 关键 benchmark > 10% 退化阻塞 commit |
| 多平台 CI matrix | 0.2.5 | macOS / Linux / Windows × x86_64/arm64 全绿 |
| 项目级 CI 模板 | 0.2.5 | `z42c new` 自带 GitHub Actions / GitLab CI 模板 |
| Release 自动化 | 0.2.6 | git tag → 跨平台 binary + zpkg 自动产出 |
| 跨平台 SDK package 分发 | 0.2.6 | 13 个 per-arch SDK 包（desktop × 5 / iOS × 3 / Android × 4 / wasm × 1）；统一 `bin/libs/native/manifest.toml` 形态（examples 已于 2026-06-20 移出发行包）；详见 [embedding.md §11.9](design/runtime/embedding.md#119-分发-package-形态per-arch-flat2026-05-13-define-package-layout) |
| 跨 mode 一致性 CI | 0.3.0 | interp / JIT 同测试集结果一致 |
| `z42b test` GREEN 门禁 | 0.3.13 | stdlib + 用户代码 z42 测试纳入 GREEN（z42b test GA，Rust test-runner 退役）|
| `z42b bench --diff` + e2e 硬门禁 | 0.4.x（B 流）| 独立 `z42.bench` 包 + `z42b bench` GA；z42 代码 bench 进 perf CI，>10% 退化真正 fail PR + 自动 diff 评论 |
| `z42-doc` 自动发布 | 0.4.x（L 流 0.4.6）| 标准库 doc comment → 静态站点 |
| LSP 集成测试 | 0.5.7 | LSP server 协议级 conformance test |
| DAP debugger conformance | 0.8.7 | VS Code / JetBrains 调试 |
| WASM target CI | 0.9.7 | VM 编译为 WASM + headless 浏览器跑 |
| 跨 mode bench 对比 | 0.10.x | interp / JIT / AOT 三模 bench 报告 |
| 跨架构 perf 矩阵 | 1.0-rc1 | x86_64 / arm64 / wasm32 perf 进 release notes |

### 多平台支持矩阵

| 平台 | 编译器 | VM | NativeAOT | 起始版本 |
|------|:---:|:---:|:---:|:----:|
| macOS x86_64 / arm64 | ✅ | ✅ | ✅ | 0.2.5 |
| Linux x86_64 / arm64 | ✅ | ✅ | ✅ | 0.2.5 |
| Windows x86_64 | ✅ | ✅ | ✅ | 0.2.5 |
| Windows arm64 | ✅ | ✅ | ⚠️ rc | 1.0-rc2 |
| WASM (wasm32-wasi) | — | ✅ VM only | — | 0.9.7 |
| iOS / Android | — | 🔬 实验 | 🔬 实验 | 1.x+ |
| 嵌入式（no_std）| — | 🔬 实验 | — | 1.x+ |

### Toolchain 矩阵

| 工具 | 用途 | 起始版本 | GA 版本 |
|------|----|:----:|:----:|
| `z42c` | 编译器驱动（build/check/run/test/bench/fmt/clean/disasm/explain/new/init/doc）| 当前 | 0.4.x |
| `z42vm` | VM 运行时 | 当前 | 0.9.x |
| `z42-fmt` | 代码格式化 | 当前 | 0.2.4 |
| `z42-doc` | API 文档生成 | 0.4.x（L 流 0.4.6）| 0.4.x |
| `z42-lsp` | Language Server Protocol | 0.5.7 | 0.6.7 |
| `z42-lint` | 静态检查 | 0.6.7 | 0.7.x |
| `z42-dap` | Debug Adapter Protocol | 0.8.7 | 0.9.x |
| `z42up` | 版本管理工具 | 1.0-rc6 | 1.0 |
| `z42-pkg` | 包注册表客户端 | 1.x+ | 1.x+ |

### GREEN 标准演进（任一时点 = 该时点之前所有项的累积）

| 起始版本 | 新增 GREEN 项 |
|:------:|------|
| 当前 | `cargo build`（z42vm）+ `xtask test`（z42c 自举 + e2e + cross-zpkg + stdlib）全绿 |
| 0.2.3 | Perf CI |
| 0.2.5 | 多平台 CI matrix |
| 0.3.10 | z42c-selfhost byte-identical gate（7 子系统逐字节对账）+ compile-perf ≤3× C# |
| 0.3.13 | `z42b test` GA（z42 原生测试运行器，Rust test-runner 退役）+ CI 三平台 GitHub Checks（WASM / iOS Simulator / Android Emulator）全绿 |
| 0.3.14 | workload B2 `z42 workload install/list/remove` 端到端绿 |
| 0.4.x（B 流）| `z42b bench --diff` 通过 + e2e bench >10% 退化硬门禁 fail PR |
| 0.4.x（S 流）| `params`/`init`/索引器/命名实参/`partial` golden 全绿 |
| 0.4.x（G+L 流）| 泛型实例化 + 泛型反射三件套绿 + JSON `Deserialize<T>` serde 用例通过 |
| 0.4.x（L 流）| `z42-doc` 无错 + CLI 校验/补全 [Test] 通过 |
| 0.5.0 | 跨 zpkg 反射元数据一致性 |
| 0.5.7 | LSP conformance |
| 0.6.7 | `z42-lint` 零警告 |
| 0.8.6 | 多线程压力测试（race detector）|
| 0.8.7 | DAP conformance |
| 0.9.7 | WASM target build & test |
| 0.10.0 | philosophy §9 五指标自动化基线 |
| 1.0.0 | C# bootstrap 删除后 z42c-selfhost 唯一编译器全绿 + 跨架构 perf 数字 |

---

## 实现里程碑（pipeline 维度）

| 里程碑 | 内容 | 所属阶段 | 状态 |
|--------|------|:-------:|:----:|
| M1 | Lexer + Parser | L1 | ✅ |
| M2 | TypeChecker（L1 特性全覆盖）| L1 → L2 | ✅ |
| M3 | IR Codegen → `.zbc`（L1 特性全覆盖）| L1 → L2 | ✅ |
| M4 | VM Interpreter（L1 特性全覆盖）| L1 | ✅ |
| M5 | VM JIT（Cranelift，L1 特性）| L1 → L2 | ✅ |
| M6 | 工程支持 + 测试体系 + `.zbc` 格式稳定 | L2 | ✅ |
| M7 | VM 元数据 + 标准库基础（core/io/collections）| L2 | 🟡 stdlib 基础已广；反射元数据 → 0.3.x C 主线 |
| M8 | TypeChecker + Codegen 扩展（L3 特性）| L3 | 🟡 部分（泛型 / lambda / delegate 提前）|
| M9 | VM AOT（**cranelift-AOT**，复用 JIT 翻译 + cranelift-object；非 LLVM，2026-06-21 改向，见 [aot.md](design/runtime/aot.md)）| L3 | 📋 |
| M10 | 自举（Self-hosting，7 子系统 byte-identical）| L3+ → 0.3.x | 🚧 进行中（B0 骨架 + 构建管线落地 2026-06-07 [scaffold-z42c-selfhost](spec/changes/scaffold-z42c-selfhost/)；架构见 [self-hosting.md](design/compiler/self-hosting.md)；core/syntax 等后续）|

---

## 待裁决问题（Q1–Q18）

> 以下问题在对应版本启动 spec 时由 User 裁决；提前列出避免实施时阻塞。

| # | 版本 | 问题 |
|:--:|:----:|-----|
| Q1 | 0.3.3 | GC v1 放 0.3 还是延后到 0.8 与多线程一起？（暂定方案：0.3）|
| Q2 | 0.4.6 | `z42.test` 注解风格：`[Test]`（C#）vs `test "name" {}`（Zig）？（推荐 C# 风）|
| Q3 | 0.5.4 | Trait 与 interface 是否完全等价？同一类型可同时实现两者？|
| Q4 | 0.6.0 | 闭包变量捕获：值捕获 vs 引用捕获 vs 显式标注？|
| Q5 | 0.6.3 | 引入 `let` 后是否提供 `var → let` codemod？|
| Q6 | 0.7.1 | `Option<T>` 与 `T?` 是否可隐式互转？编译器层视为同一类型？|
| Q7 | 0.8.5 | 数据竞争预防：Send/Sync trait 还是注解 + 编译器分析？|
| Q8 | 0.9.5 | VM 组件化粒度：cargo feature 还是构建时 build profile？|
| Q9 | 0.10.x | 性能强化 9 个 patch 独立发布还是合并 0.10.0 单次？|
| Q10 | 1.0 | AOT 是否必须卡 1.0？（备选：1.0 = 自举 + 稳定，1.1 = AOT）|
| Q11 | 0.2.5 | 多平台 CI 选 GitHub Actions matrix 还是自托管 runner？arm64 主机如何获取？|
| ~~Q12~~ | ~~0.2.5~~ | ~~Release artifact 命名~~ — 已裁决 2026-05-14：`z42-<version>-<rid>.{tar.gz\|zip}`（9 RID；windows-x64 用 zip，其余 tar.gz；含 SHA256SUMS）。详见 [archive/2026-05-14-add-release-automation/design.md](../docs/spec/archive/2026-05-14-add-release-automation/design.md)。|
| Q13 | 0.5.7 | LSP server 用 .NET（复用编译器）还是 Rust（复用 VM）？|
| Q14 | 0.8.7 | DAP debugger 多线程暂停语义：单 thread 还是全部？JIT/AOT 如何 step？|
| Q15 | 0.9.7 | WASM 下 GC：等 wasm-gc proposal 还是自实现 wasm-internal GC？|
| Q16 | 0.9.8 | 嵌入式 ~200KB 平台基准（cortex-M4 / esp32 / RISC-V？）|
| Q17 | 1.0-rc6 | `z42up` 用 Rust 还是等自举后用 z42 自身实现？|
| Q18 | 1.x+ | 包注册表中心化（crates.io 模式）还是去中心化（go modules / git URL）？|

---

## Deferred Backlog Index

> 所有显式延后特性的横向索引；条目正文存于对应 design doc 的 "Deferred / Future Work" 段。新增延后项时：① 在对应 design doc 加条目 ② 在本表加索引行。规则见 [`.claude/rules/philosophy.md`](../.claude/rules/philosophy.md#延后特性管理必须遵守) "延后特性管理"。

### 设计期延后

| 特性 | 描述 | 在哪里 |
|------|------|------|
| L3-G3a 关联类型 | `where T: IAdd<Output=T>` + zbc 扩展 + 运行时校验。**parser 无 `Name=Type` 解析，当前未实现**（generics.md 曾按已实现描述，2026-09-05 订正） | [language/generics.md](design/language/generics.md) |
| where 约束：接口类型实参匹配（where-constraint-future-type-arg-matching）| 接口约束 v1 **只比裸名**——`IEquatable<string>` 也满足 `where T : IEquatable<T>`。与运行期同口径故无分歧；裸名匹配还顺带消掉 F-bounded 自引用（`INumber<T> where T : INumber<T>`）的无限递归 | [book: generic-constraints.md](book/src/language/generic-constraints.md) 已知限制 §2 |
| where 约束：跨包持久化（where-constraint-future-crosspkg）| **跨包泛型实例化的约束 100% 不校验**（连基类/`class`/`struct` 也不）——`ClassConstraints` 唯一写入点只遍历本包 CU 的 ClassDecl。补它需给 `ExportedClassZ` 加约束字段 → 踩 bootstrap-seed 第二根轴（stdlib API 面），须卡 nightly 节奏；另需先定键规则（本地裸类名 vs 导入侧 arity-mangle `Name$N`）。**「基元 wrapper 归一偏差 → `Dictionary<int,int>` 编不过 → 自举链断」这条 🔴 风险在此阶段才真正暴露** | [book: generic-constraints.md](book/src/language/generic-constraints.md) 已知限制 §1 |
| where 约束：运行期 flag 位接活（where-constraint-future-runtime-flags）| ZbcWriter 只写 bit3+接口名列表，运行期 `validate_type_arg_constraint` 的 class/struct/base/new()/enum 五个分支是**死代码**。置全 flag 位即可接活（zbc 格式无需 bump，位早已规约）。硬前置 ZbcReader 补读 bit2（已随 #475 落地） | [book: generic-constraints.md](book/src/language/generic-constraints.md) |
| where 约束：推断调用与顶层函数（where-constraint-future-inferred-method-args / -toplevel-func）| 方法级约束只在**显式**写类型实参时校验（`Max<int>(a,b)` 校验、`Max(a,b)` 不校验）；顶层 `FuncDecl` 的 `where` 完全不校验 | [book: generic-constraints.md](book/src/language/generic-constraints.md) 已知限制 §3/§4 |
| where 约束：func 类型约束（where-constraint-future-func-constraint）| `where T : Func<int,R>` —— E0422/E0423 已定义但从未发出。⚠️ CallEmitter 靠该约束把参数当 func 值走 CallIndirect，改动需谨慎 | [book: generic-constraints.md](book/src/language/generic-constraints.md) 已知限制 §5 |
| 跨包 / 多文件负例门（where-constraint-future-crosspkg-negative-gate）| `SemanticDump` 只覆盖**单文件语义**诊断；E0404（cross-zpkg internal）、E0451 等仍靠手工 fixture + README 描述步骤，无自动门 | [archive/…complete-where-constraints/design.md](spec/archive/2026-09-05-complete-where-constraints/design.md) §7 |
| 协议方法一等重载（protocol-overload-first-class）| 对象协议方法（`Equals`/`ToString`/`GetHashCode`/`GetType`/索引器）恒裸名注册（VM/DepIndex 裸名 vtable 单槽派发）→ 同名重载 RegKey 塌缩、只一个可派发，无法承载「行为发散」的协议重载（`Std.String` 两 `Equals` 同 native 故可用）。正解=C# 模型：解耦运行时规范协议槽 vs 调用点完整重载集（VM+编译器工程，可能连带 vtable/DepIndex/格式）。触发：出现真实发散用例（罕见，契约本要求 Equals(object)/Equals(T) 一致）。暴露于 fix-partial-protocol-overload-e0433 | [book: source-compile.md](book/src/compiler/source-compile.md) Deferred 段 |
| TLAB slot 级复用（gc-tlab-slot-reuse）| chunk 独占 TLAB 绕过 region slot 级 free_list；partial-live chunk 的零散死槽暂不被 TLAB 复用（chunk 级回收已做）。触发：live set 稳定但堆随 GC 轮次单调涨 → per-thread free-slot cache | [book: GC TLAB](book/src/runtime/gc-tlab-chunk-exclusive.md) Deferred 段 |
| 编译器重阶段并行化（compiler-parallel-heavy-phases）| 真正并行加速前置：把 parse/typecheck/codegen 做成 per-file 并行（当前仅 source-read+SHA 并行 → Amdahl 受限）。TLAB 已铺零锁地基；此为编译器侧 change | [book: GC TLAB](book/src/runtime/gc-tlab-chunk-exclusive.md) 性能门段 |
| json-serde 集合类型（json-serde-future-collections）| ~~`List<T>` + `Dictionary<string,V>`~~ ✅ add-collection-serde（反射-only + 2 小 runtime 反射修复）。剩：`Dictionary<K,V>` 非字符串键（→ array-of-pairs）、`Set`/`Queue`/`Stack` | [changes/add-collection-serde/design.md](spec/changes/add-collection-serde/design.md) Deferred 段 |
| json-serde enum/nullable/char（json-serde-future-enum-nullable-char）| enum（名/底层值）、`T?`、char 的 serde 映射 | [changes/add-json-serde/design.md](spec/changes/add-json-serde/design.md) Deferred 段 |
| json-serde 命名策略（json-serde-future-casing-policy）| camelCase↔PascalCase 自动命名策略（workaround：逐成员 `[JsonProperty]`）| [changes/add-json-serde/design.md](spec/changes/add-json-serde/design.md) Deferred 段 |
| ~~serde 公开反射 API 下沉（json-serde-future-public-reflection-api）~~ | ✅ **add-array-property-reflection-api**（2026-08-24）：`Std.Array` 照搬 C# `System.Array`（静态 `CreateInstance` + 实例 `GetValue`/`SetValue`/`.Length`）+ `PropertyInfo.GetCustomAttributes`/`GetAttribute`；z42.json 改用并删自有 extern。零格式 bump（原 gen0 顾虑经核实对 stdlib 源不成立，axis② 豁免 + workspace 自洽解析）| [archive/2026-08-24-add-array-property-reflection-api/](spec/archive/) |
| 数字/Unicode 转义（escape-future-numeric-unicode）| `\uXXXX` / `\xXX` / `\0` 八进制扩展 / `\UXXXXXXXX` 解码；需 hex/oct 解析 + 码点越界诊断。当前（reject-invalid-string-escape 后）这些转义报 E0102，workaround = 源码写字面字符或用 raw 串 | [changes/reject-invalid-string-escape/design.md](spec/changes/reject-invalid-string-escape/design.md) "Deferred / Future Work" 段 |
| REPL 泛型返回续读（repl-completeness-future-generic-return）| 泛型返回类型函数头 `List<int> foo()` 被 Classifier 漏判为表达式 → 无法多行续读；需 Classifier 泛型感知或 parser submission 模式 | [archive/2026-08-08-add-repl-parser-completeness/design.md](spec/archive/) Deferred 段 |
| 闭包档 A 完整版 | 任何不逃逸 closure 栈分配（当前仅单变量子集）| [language/closure.md](design/language/closure.md) |
| 闭包档 B 完整版 | 单态化 + 泛型形参标注（当前仅 alias 子集）| [language/closure.md](design/language/closure.md) |
| 闭包档 C send 派生 | 与 concurrency 实施一起做 | [language/closure.md](design/language/closure.md) |
| Static abstract iter 2+ | 类型级访问（`T.Zero` / `T.Parse(s)`）| [language/static-abstract-interface.md](design/language/static-abstract-interface.md) |
| 重载键稳定化（最终方案 A）| 消除「加/删重载 → re-mangle 现有方法键」的 bootstrap 敏感性：`SymbolCollector.regName` 从「兄弟集相关」（唯一→裸名 / 多 arity→`Name$arity` / 同 arity→`Name$arity$types`）改为**一律全签名 mangle**（键 = 自身签名纯函数、兄弟无关；协议豁免名 ToString/Equals/… 保持裸名 VM 硬查）→ 键永久稳定、未来加重载零 bootstrap 处理。代价大：全局改键（巨大字节 diff）+ 自指两代自举过渡（链接约定级）+ 硬编码名审计（compiler+VM 反射/well-known/DepIndex）。**2026-07-12 实测关键发现**：给唯一方法加重载触发的 re-mangle **不被现有 bootstrap 自愈**——`build stdlib` 本地即崩（seed driver 打新键 z42.io → `undefined Path.Join`），且**无格式版本 bump 触发不了 ci-bootstrap 两代自举**（那只认 zbc/zpkg minor 差）。故「params 两阶段」并非轻量逃生——它撞同一堵墙。要落地这类「z42c 消费的 stdlib 方法加重载」，须**方案 A** 或**随格式 bump 搭两代自举**或**换名兜底**（如变长版另起名/复用 `Combine(params)`）。低频 → 暂缓 | [compiler_review.md](compiler_review.md) §二·派发键稳定化 |
| compiler-review P1 God-Class 拆分（续）| TypeChecker 步骤 2-4（MemberResolver/StmtBinder/ExprTyper 收敛 Facade——step1 抽 OverloadBinder 已归档 2026-07-12，EmitContext 式 mediator 拆法不动点 7/7 验证）+ Parser(1739) + IrGen/ExprEmitter/FunctionEmitter | [compiler_review.md](compiler_review.md) §一/§七 |
| compile-once 正式模型 (CO-D1..D4) | test 腿全消费 current-sdk / 成对分代 gen1-3 / 三发布门+cross-bootstrap / 本地分阶段命令 —— 经实测低性价比（多为 compute 非墙钟），按需新开 change（如 format-bump 安全要 CO-D2）| [archive/2026-06-30-compile-once-toolchain/tasks.md](spec/archive/2026-06-30-compile-once-toolchain/tasks.md) Deferred 段 |
| 逃逸分析栈分配 future（escape-stack-future-*）| ① JIT 侧 arena 落地（v1 JIT 忽略 flag）② ~~跨过程参数逃逸摘要~~ **已落地**（`add-crossproc-escape-summary`：模块不动点 `ParamEscapeTable`，CallInstr/ObjNew 实参按 callee 摘要判、`_ctorLeaksThis` 并入；VCall/去虚化 + IsInstance/Convert 放宽仍留后）③ 标量替换（对象炸成寄存器，第二种 lowering）④ ~~scope/回边级 arena 复位（热循环内累积）~~ **由 loop-alloc-reuse 主攻**（见下行；hoist+复用把循环内每迭代 new 降到 1 次分配，实测 interp 2.91×/jit 4.09×）；arena 回边复位作证不出时的栈侧兜底仍可后补 | [add-escape-analysis-stack-alloc/design.md](spec/changes/add-escape-analysis-stack-alloc/design.md) "Deferred / Future Work" 段 |
| 循环内分配 hoist+复用 future | ① `ArrayNewLit`（字面量元素复用需元素写手术）② 嵌套循环全外提（v1 只 hoist 到内层 pre-header，处理序决定、仍正确收益略逊）③ 数组动态下标 / 变长 Size / 多块使用 的复用 ④ scope/回边 arena 复位兜底「证不出但迭代内局部」的栈对象 | [archive/2026-08-06-add-loop-alloc-hoist-reuse/design.md](spec/archive/2026-08-06-add-loop-alloc-hoist-reuse/design.md) Deferred 段 |
| readonly 字段读优化 future（add-readonly-fields-opt）| ① **跨 zpkg 导入字段 readonly**（需 zbc/zpkg 格式 bump 把 readonly 位写进 `IrFieldDesc` + ZbcWriter/Reader + TsigReconcile + ImportedSymbolLoader；v1 只同模块）② **非 `this` 接收者的 LICM 外提**（形参/局部 readonly 字段读——需非空/支配分析证无 NPE 时机漂移）③ `readonly struct` / non-null 类型（各自独立 change）| [archive/2026-08-06-add-readonly-fields-opt/design.md](spec/archive/2026-08-06-add-readonly-fields-opt/design.md) Deferred 段 |
| 纯函数调用优化 future（add-pure-call-opt）| ① **跨 zpkg pure**（imported 函数纯度——`IrFunction.Attrs` 已序列化理论可读 / 或跨包摘要；v1 只同模块保守非纯）② **去虚化后 VCall 判纯**（final 类/单态化——实例方法现为 VCall、pure-call 覆盖不到；**性价比高的下一步**：去虚化→变直接 Call→能判纯，大幅扩覆盖）③ **含分配的确定性构造函数标量替换**（需身份分析）④ **放宽 Div/Rem**（可证非零除数）⑤ `pure` 关键字作可验证契约（文档+防回归+跨包载体） | [archive/2026-08-06-add-pure-call-opt/design.md](spec/archive/2026-08-06-add-pure-call-opt/design.md) Deferred 段 |
| const 编译期常量 future（add-const-keyword）| ① **跨 zpkg const**（const 值写进导出元数据 → 格式 bump；v1 只同模块，const 无字段元数据别包看不到）② **跨类/跨作用域 const 初始化器引用**（v1 字段初始化器只引同类已定义 const、局部只引作用域内局部 const）③ **const 引用 enum 成员 / const 数组 / const 对象**（v1 仅原始类型常量）④ **`ExcCount>0` 函数的死块移除**（v1 只折 br.cond→br 不移块；需异常边纳入 CFG）| [archive/2026-08-07-add-const-keyword/design.md](spec/archive/2026-08-07-add-const-keyword/design.md) Deferred 段 |
| struct 堆内联 P3b follow-up（add-struct-heap-inline）| **已落地**：① struct 真内联进**堆对象字段**（`class C{ Point pt; }`；D1-a 基元字节内联 `struct_bytes` + 引用叶子 `struct_refs` 侧表 + 复用 `StructFieldGetPrim/SetPrim` 对象 base 路线 α + GC scan/`write_barrier_field` 复用侧表 + 格式 wire 内联字段表 zbc1.32/zpkg0.37）② **`struct[]` 值类型数组元素 codegen**（add-struct-array-codegen：`array_get` 产 `StructRefHeap` 句柄 + `array_new/lit` 造 `StructBytes` backing + `_emitIndex`/`_structChainRoot` 出句柄/拷贝——格式中立，golden `struct_array.z42` 验）③ **class 实例方法返回 struct**（add-struct-method-return：`_emitCall` instance 三派发路径追加 sret 隐藏实参；object VCall 按 vtable slot 派发不破——格式中立，golden `struct_heap_inline.z42`(GetPt) 验）④ **foreach over struct[]**（add-struct-foreach：`as_cast` 加 `StructRefHeap` 臂 → `copy_array_elem_out` 拷元素到帧 arena StructRef，runtime-only 格式中立，golden `struct_array.z42` foreach 段验）⑤ **JIT 值路径 P5-A**（add-struct-jit-value-path：struct 指令 emit 为 helper call 操作共享 arena、复用 interp `*_val` 核心 + `JitFrame.frame_id` 惰性分配 + `jit_array_new/get` StructBytes/StructRefHeap + `jit_as_cast` 拆箱——含 struct 的函数不再整体 bail→interp，格式中立，golden `struct_jit.z42` `--mode jit` 验）⑥ **跨包 struct 值语义 P4a**（add-crosspkg-struct-value-semantics：`ImportedSymbolLoader` `nct.IsStruct = !cl.HasBase` 复用生产方 `HasBase=!isStruct` 编码——不新增 stdlib API 零 bootstrap 越界，消费方重算布局与生产方逐字节一致，修 imported struct 被当引用类型的 blob-bounds 崩，格式中立，golden `struct_cross_pkg` transitive 验）⑦ **装箱引用身份 + struct 字段反射 P4b**（add-boxed-struct-identity：`BoxedStruct` 载荷从值语义 `Box<BoxedStructData>` 改共享 `GcRef<ScriptObject>`——对齐 C# 引用身份、复用 region_object 零 GC 改动；`FieldInfo.GetValue/SetValue` 反射装箱 struct 字段——Rust 复刻 `_compute` 布局 + 三层校验，格式中立，golden `reflection/struct_field` + 单测验）⑧ **对象内联 struct 字段反射 P4b-B**（add-object-inline-struct-reflection：`FieldInfo.GetValue/SetValue` 读写 `class C{ Point pt; }` 内联 struct 字段——复刻**类级**内联布局 `compute_class_inline` + 共用 `snapshot_struct_leaf`/`write_struct_leaf`，格式中立，golden `reflection/struct_field` 扩对象内联+嵌套用例 + 3 单测验）。**剩余**：⑨ **JIT 原生内联字节访问**（P5-B，现 helper 桥接=interp 速度，待 benchmark 驱动）| [book struct-value-semantics](book/src/runtime/struct-value-semantics.md) 「堆内联」+「JIT 值路径」+「跨包 struct」+「装箱引用身份 + struct 字段反射」+「对象内联 struct 字段反射」段 + [archive/…-add-struct-heap-inline](spec/archive/) |
| sealed 去虚化 v1 边界外（add-sealed-devirt-future）| **已落地**：本地非泛型（`add-sealed-devirt`）+ **imported 非泛型**（`extend-sealed-devirt-imported`：`_devirtQualifiable` 认 `ImportedClassNs`；imported 定义类经 `Deps.Statics` 校验 FQ 真实发射——排除 TSIG 展平的继承方法）+ **sealed override + 泛型 sealed**（`extend-sealed-devirt-more`：`DevirtReceiverClass` 放宽入口（含解包 `Z42InstantiatedType.Def`）+ `ResolveSealedTarget(…, classSealed)` declClass 处门控 `classSealed ‖ ms.IsSealed` + `_classShortName` 的 `$N` 条件 arity-mangle）。`Opt.Devirt` bit11。**剩余回落 VCall**：非虚/接口/cast-unknown（既有守卫）、流敏感型别精化（仍按静态声明类型） | [book sealed](book/src/language/sealed.md) 「去虚化」+「Deferred」段 |
| 用户自定义转换 future（user-conversions-future-*）| ① **`as`/`is`/模式匹配接入用户转换**（C# 硬伤②；可失败语义需额外协议，v1 只 `(T)x` 显式 + 隐式上下文）② **标准转换 + 用户转换组合链**（v1 精确 (源,目标) 匹配、比 C# 更可预测；多跳由 ③ 走中间类型诊断引导手写 `(C)(B)x`）| [archive/…-add-user-conversions/design.md](spec/archive/) "Deferred / Future Work" 段 |
| 编译器指纹自动化（fingerprint-future-auto-buildid）| A 方案（`add-compiler-fingerprint-cache`）手动 `CompilerFingerprint` bump 靠人肉；B 方案 = driver 经 `Z42_HOME` 聚合自身 `programs/z42c/*.zpkg` 的 `build_id`（BLAKE3-128）入 cache key，编译器一变即自动失效、免 bump。暂缓：多启动路径（cold/warm/REPL/z42b/wasm）下 `Z42_HOME` 解析自身产物的验证面大，发布前不值得。触发：0.4.x 尾 build-orchestration 阶段 | [archive/2026-08-11-add-compiler-fingerprint-cache/tasks.md](spec/archive/2026-08-11-add-compiler-fingerprint-cache/tasks.md) 备注 |
| CO-D1 收尾：统一 toolchain artifact | 让**所有**消费者（host-package/platform/windows，非仅 test）改吃 `current-sdk`（build sdk，需补 xtask 进它）→ 删 `build stage-toolchain` + `toolchain-<os>` artifact（与 `build sdk`/`current-sdk` 重复：差集仅 xtask/vm/apphosts/布局）。省 ~6 job 各一条 bootstrap 之外的重复；纯 CI 改动、只能 CI 验（redesign-xtask-test 期间评估：stage-toolchain 当前仍在关键路径，不可裸删） | [archive/2026-06-30-compile-once-toolchain/tasks.md](spec/archive/2026-06-30-compile-once-toolchain/tasks.md) Deferred 段 |
| z42vm JIT cdylib 拆分 | 把 cranelift JIT 拆成可 dlopen 的 `libz42_jit.dylib`（z42vm 6M→~3.5M）；ROI 低（拆 ~3M / 整包 ~70M，中高工作量）2026-06-21 暂缓 | [toolchain/runtime-workload-distribution.md](design/toolchain/runtime-workload-distribution.md#deferred--待-spec-细化) |
| 组件化运行时 | libz42 基座 + interp/jit/aot/gc/debug 组件；static/dynlink/dlopen 三粒度 + 切换语义；嵌入按需链接 | [runtime/componentized-runtime.md](design/runtime/componentized-runtime.md) |
| 分层执行 | interp/JIT 各自内部分层 + OSR/deopt + 低层回收 + 引用诊断 + hot-reload 共用基建 | [runtime/tiered-execution.md](design/runtime/tiered-execution.md) |
| IR 优化与特化 | 编译期优化 tier0 基线 + intrinsic 表（编译期折常量 + 引擎内联，硬编码纯度）；`"sss".Length` 折叠 | [runtime/ir-specialization.md](design/runtime/ir-specialization.md) |
| 加载上下文（ALC 式） | zpkg 重载/卸载/回收（含内部 metadata/缓存池）；惰性 GC 卸载 + 保留根诊断 + 缓存不自钉铁律 | [runtime/load-context.md](design/runtime/load-context.md) |
| 诊断与跟踪 | 事件（编译/类型/GC/deopt/context）+ 计数（counter/gauge/histogram）+ 时间（per-函数编译耗时）；fire() 近零成本门控 + perfetto 输出 | [runtime/diagnostics.md](design/runtime/diagnostics.md) |
| 统一 safepoint/STW + 精确 GC 契约 | GC safepoint 泛化为 OSR/卸载/hot-reload 共用；线程状态（InNative=安全）；精确 GC = GC map@安全点 + 派生指针受控（ALC 卸载前提） | [runtime/safepoint.md](design/runtime/safepoint.md) |
| 对象与值表示 ABI | 隐式 Value/对象 ABI 固化（repr(C)+tag 表）；统一对象头去 native；字符串改 GC；移动/分代预留（gc_word/forwarding/card table/pin 区）；TypeDesc 留 context-arena | [runtime/object-abi.md](design/runtime/object-abi.md) |
| ✅ 引用压 8B / `Value` 24→16B（B-radical 子目标，2026-08-11 提，**2026-08-15 落地 `unify-object-byte-layout` PR-3~5**）| 引用改平台指针大小（GcRef 16B→8B：**采路 A** 标记指针塞窄 generation 保非移动 GC；String `Arc<str>` 胖指针 16B→细指针 8B 长度进头；**FuncRef `Box<str>`→`Str` 8B**）→ `Value` 最大 payload 8B → enum 24B→16B（全 VM 密度/cache 33%）。**收益在密度非 native 交互**。全 VM 横切（JIT stride 24→`size_of::<Value>()`+`size_of::<Value>()==16` 编译期 pin）；不在 struct P3b。~~残留 follow-up：string 全 GC 化~~（✅ 已由 `unify-gc-heap` 完成，见下行） | [runtime/object-abi.md §2.1](design/runtime/object-abi.md) |
| ✅ 统一 GC 堆（B-radical，**2026-08 落地 `unify-gc-heap` PR-1~5**）| string / closure / array backing 三类变长 payload 全收进单一 GC 变长块 region（`gc/var_region.rs`，A' 分配器）：PR-1 分配器原语 → PR-2 closure → PR-3 array backing → PR-4 string（ambient 堆 + lazy per-ctx interning + `Value::Str(VarGcRef)`）→ PR-5 收敛（fn_name 迁 GC Str 删 closure drop-glue + `visit_gc_children` 单一访问器 + 删 interned_strings 死代码）。消除 Arc/Box/外部 Vec 与 GC 的双重管理，为移动/压缩/去重 GC 铺路。前置 = 引用压 8B（上行） | [runtime/gc.md「变长块堆」](design/runtime/gc.md)、[object-abi.md §5](design/runtime/object-abi.md) |
| AOT 后端 | cranelift-AOT（复用 JIT 翻译 + cranelift-object，非 LLVM）；AOT+JIT+interp 混合（.NET R2R/ART 模型）；host 交叉编译；精确 GC stack map | [runtime/aot.md](design/runtime/aot.md) |
| ref local / return / field / struct | parameter-modifiers D1-D4 | [language/parameter-modifiers.md](design/language/parameter-modifiers.md) |
| StackTrace / 构造器重载 / 字段 ? 标注 / self-assign | exceptions Phase 1 限制 | [language/exceptions.md](design/language/exceptions.md) |
| Layer 3 用户定义 operator/keyword | customization 第三层 | [language/customization.md](design/language/customization.md) |
| 元编程 / 编译期代码生成 | 同语言宏（VM 编译期执行 + 类型化 AST + quote/splice）；分层 derive→模板宏→变换宏；先做 derive（复用反射）| [language/metaprogramming.md](design/language/metaprogramming.md#deferred--分期诚实这是语言里最难的几件事之一) |
| foreach IEnumerator 路径 | 升级为接口 dispatch（当前仅鸭子协议）| [language/iteration.md](design/language/iteration.md) |
| 自定义 body / init-only / expression-bodied property | properties 未支持子集 | [language/properties.md](design/language/properties.md) |
| `Type : MemberInfo` 层级对齐 | 统一 Type 不拆 TypeInfo（2026-06-09 已定）；但 Type 当前非 MemberInfo 子类、不在 Std.Reflection——对齐留待嵌套类型反射 / 自举镜像时 | [language/reflection.md](design/language/reflection.md#deferred--future-work) |
| 继承的静态字段反射 | `GetFields()` 含静态已落地（2026-06-10）但仅声明类自身；继承静态需沿 base 链聚合 `static_fields` | [language/reflection.md](design/language/reflection.md#deferred--future-work) |
| 嵌套泛型 type-args / 实例 GetGenericArguments | `IsGenericTypeDefinition`/`GetGenericTypeDefinition` + typeof 携 args 已落地（2026-06-16，zbc 1.18 Typeof opcode）；剩 `typeof(Box<Map<K,V>>)` 嵌套递归 + `new Box<int>()` 实例 `obj.GetType()` 两路径统一 | [language/reflection.md](design/language/reflection.md#deferred--future-work) |
| `Type.IsEnum` + 接口成员/transitive | `IsClass`/`IsInterface` 已落地（2026-06-16，zbc 1.19 flags bit4 + 接口产最小 TYPE 条目）；`IsEnum` **已落地**（2026-07-09 add-enum-type-metadata，zbc 1.22 flags bit5 + enum 成员块）；**接口成员枚举已落地**（2026-07-20 add-interface-member-reflection，纯 runtime surface zbc 1.28 接口方法块）；接口继承接口方法 / 数组 IsClass 续作 | [language/reflection.md](design/language/reflection.md#deferred--future-work) |
| reconcile-tsig CI gate 布线 | `reconcile-tsig` verb 全 29 包本地 OK（unify P2，2026-07-11）；接入 `xtask test` 需 toolchain 锁（fix-bootstrap 持有）→ P3 全面切换后 GREEN gate 天然覆盖 | [compiler/project.md](design/compiler/project.md#tsig-对账重建unify-type-metadata-p22026-07-11) |
| 非字面量参数默认值的值 | `ParameterInfo.DefaultValue` 只折字面量（add-param-metadata，2026-07-10，zbc 1.25）；常量表达式/enum 成员默认值需常量折叠器 | [language/reflection.md](design/language/reflection.md#deferred--future-work) |
| Tier 2/3 完整 interop | manifest reader / 源生成 / symbol resolution | [language/interop.md](design/language/interop.md) |
| 整体 L3 concurrency | async/await / Future / Send-Sync / 调度器 | [runtime/concurrency.md](design/runtime/concurrency.md) |
| hot-reload 签名变更 + 跨模块 | 签名变更检测 / 跨模块 reload 故事 | [runtime/hot-reload.md](design/runtime/hot-reload.md) |
| 完整 JIT 指令映射 + 性能基准 | jit.md 待补 | [runtime/jit.md](design/runtime/jit.md) |
| GC handle Phase 3+ | Pinned / WeakTrackResurrection / 多线程 barrier | [runtime/gc-handle.md](design/runtime/gc-handle.md) |
| launcher 下载/install/self-update (P2) | `z42 install/uninstall/self update` + 每平台×版本发布点 + 校验（P1 用 `z42 link` 本地注册） | [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| launcher app 版本声明格式 | zpkg `META.toolchain_version` vs `runtimeconfig.json` sidecar 未定；分发(P2)时才需 | [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| z42c 裸脚本→Exe-zpkg | 原 launcher phase 0.5；现以 mini-project(`kind="exe"` toml) workaround，ROI 低 | [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| `ICompiler` 抽中立微库 | z42b 编译接口暂置 `z42.build`；后抽中立微库使编译器核心（z42c）不依赖整个 build 框架 | [toolchain/build-orchestrator.md](design/toolchain/build-orchestrator.md#deferred--待-spec-细化) |
| z42c stdlib 构建 jit 加速 | S3（z42c 接管 build stdlib，当前阻塞未落地）落地后：interp 重编 ~30s，jit 加速待实测 22 库 jit==interp 等价 | [compiler/self-hosting.md](design/compiler/self-hosting.md#deferred--future-work) |
| z42c 继承默认参数方法 TSIG arity | 直接定义方法已修（requiredCount 读 Param.Default）；继承自其它包的默认参数方法 re-export 需 `Z42FuncType.MinArgCount`（import 时丢失），当前 stdlib 未触发 | [compiler/self-hosting.md](design/compiler/self-hosting.md#deferred--future-work) |
| S3 剩余 2 个 z42c codegen bug | dogfood S3 余 4 stdlib test：① blake3 多块 z42c codegen ② 静态字段 mutation 不持久（diagnostics）。已修 6 bug（含 cross-ns 静态调用） | [compiler/self-hosting.md](design/compiler/self-hosting.md#deferred--future-work) |
| apphost self-contained | `--self-contained`：VM+libs 随 app 本地化（P1 仅 framework-dependent）| [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| apphost single-file | 链 `libz42_vm` + 内嵌 zpkg/libs，经 embedding C ABI 内存加载；依赖 C ABI + 碰 runtime | [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| apphost Windows checksum/Authenticode + 跨平台交叉签名 | Windows PE checksum / 在 Linux 上签 macOS apphost（需内建 Mach-O 签名器；P1 用 host codesign）| [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| apphost cwd 上行 / 富搜索配置 | P1 本地搜索仅 exe 目录上行 | [runtime/launcher.md](design/runtime/launcher.md#deferred--future-work) |
| workload install 后续（B1 命令发现 / B4 平台测试 / B5 mobile publish-run / 真机多-slice xcframework）| B2 LOCAL install + B2-4 CI release/manifest 联网装 + host gate 均已落地（2026-06-17）；剩余为后续 change | [toolchain/runtime-workload-distribution.md](design/toolchain/runtime-workload-distribution.md#deferred--待-spec-细化) |
| stdlib 剩余缺失包 | **async** 仍延后（依赖 L3 async/await 语法）；~~fs~~ ✅ / ~~os~~ ✅（合入 z42.io）/ ~~threading~~ ✅ 2026-05-20 / ~~net~~ ✅ K1-K4 2026-05-24~05-25 / ~~crypto~~ ✅ SHA-1/256+HMAC 2026-05-24~05-25。详 `docs/design/stdlib/roadmap.md` | [stdlib/roadmap.md](design/stdlib/roadmap.md) |
| split-debug-symbols 退化 trace ip+build_id | line==0 时帧追加 `+0x<ip> [build:<8hex>]`；需 VmFrame 追踪 PC | [language/exceptions.md](design/language/exceptions.md#deferred--future-work) |
| `z42c symbolicate` 离线工具 | 把 `.zsym` 应用到 crash trace 还原 file:line:col | [language/exceptions.md](design/language/exceptions.md#deferred--future-work) |
| sidecar lazy / mmap 加载 | 启动延迟敏感场景的优化路径 | [language/exceptions.md](design/language/exceptions.md#deferred--future-work) |
| sidecar 跨目录搜索 | debuginfod 风格 + 环境变量配置 | [language/exceptions.md](design/language/exceptions.md#deferred--future-work) |
| `Std.Reflection.Symbolicate` 公开 API | 程序内触发符号化 | [language/exceptions.md](design/language/exceptions.md#deferred--future-work) |
| Facade threading 测试（R8）| 等 runtime threading 模型落地后回到 platform-test-contract 补"后台 invoke + 主线程 sink"scenario | [runtime/embedding.md §12](design/runtime/embedding.md#§12-deferred明确不做的) |
| multi-arch-container-packages | multi-slice xcframework / multi-ABI AAR 卷起来发；Phase 1 选 per-arch flat（13 包），用户呼声出来再加 `z42-<v>-ios-xcframework-<config>` / `z42-<v>-android-aar-<config>` 两个 convenience 包 | [runtime/embedding.md §11.9](design/runtime/embedding.md#119-分发-package-形态per-arch-flat2026-05-13-define-package-layout) |
| per-arch-abi-feature-matrix | abi-version 升 2 后"哪些 host config 字段哪个 ABI 起可用"细粒度矩阵 | [runtime/embedding.md §11.9](design/runtime/embedding.md#119-分发-package-形态per-arch-flat2026-05-13-define-package-layout) |
| binary-package-signing | iOS xcframework / Android AAR / wasm npm publish 时 notarization / GPG / npm 2FA；Phase 1 全 unsigned，留给 Phase 4 release CI | [runtime/embedding.md §11.9](design/runtime/embedding.md#119-分发-package-形态per-arch-flat2026-05-13-define-package-layout) |
| z42 build-driver prerequisites | 用 z42 自身重写所有 `.sh` 解 Tier 1 Windows CI；阻塞 = P0 z42.os/z42.io.fs + P1 z42.crypto/z42.net + P2 z42.toml/z42.compression | [stdlib/roadmap.md "Deferred / Future Work"](design/stdlib/roadmap.md#z42-build-driver-prerequisites2026-05-13) |
| ~~pre-existing cargo test build break~~ ✅ | 已修复 2026-05-27 `f7c15058` —— 根因是 `gc::region_tests` / `arc_heap_tests::invariants` 调 `#[cfg(debug_assertions)]` 方法但模块仅 `#[cfg(test)]`。2 行 fix 把模块 cfg 收紧到 `cfg(all(test, debug_assertions))`。验证：release 673/673 + debug 716/716 全绿 | — |
| ~~URL-safe Base64~~ ✅ + ~~Base32~~ ✅ + ~~UTF-16/32~~ ✅ + ~~Crockford~~ ✅ + ~~Base32-hex~~ ✅ + Encoding streaming / Base85 | **Base64Url / Base32 已落地 2026-05-25**；**UTF-16 + UTF-32 已落地 2026-05-27** (`37b7191e`)；**Base32Crockford + Base32Hex 已落地 2026-05-25**；仅 **Encoding streaming API + Base85** 仍延后 | [stdlib/encoding.md](design/stdlib/encoding.md#deferred--future-work) |
| HMAC-SHA256 | v0 SHA-256 落地后的下一步；RFC 2104 公式 | [stdlib/crypto.md](design/stdlib/crypto.md#hmac-sha256) |
| ~~Std.Crypto.SecureRandom (CSPRNG)~~ ✅ | **✅ 已落地 2026-05-26** (add-csprng-to-crypto)；wasm32 bridge 仍延后 | [stdlib/crypto.md](design/stdlib/crypto.md#csprng-wasm32-bridgestdcryptosecurerandom-on-wasm32) |
| ~~Zip.Write~~ ✅ + Zip.CreateFromDirectory | **Zip.Write 已落地 2026-05-27** (`add-zip-write`，single-buffer 2-pass 绕过 byte[][])；仅 **`Zip.CreateFromDirectory`**（atop Zip.Write + Directory.Enumerate）仍延后 | [stdlib/compression.md](design/stdlib/compression.md#compression-future-zip-create-from-directory) |
| ~~Compression streaming decode~~ ✅ | **cdylib 流式 2026-05-27** (`add-compression-streaming-decode`) + **z42 消费端 per-chunk pull 2026-06-09** (`compression-decoder-pull-mode`) → 流式解压端到端，不再 accumulate-then-decompress | [stdlib/compression.md](design/stdlib/compression.md#compression-future-streaming-decode) |
| Brotli / xz / LZ4 | z42.compression v0 算法之外 | [stdlib/compression.md](design/stdlib/compression.md#compression-future-brotli) |
| wasm zstd | 需 WASI SDK 或 ruzstd | [stdlib/compression.md](design/stdlib/compression.md#compression-future-wasm-zstd) |
| YAML ~~anchors~~ ✅ / ~~tags~~ ✅ / ~~multi-line~~ ✅ / ~~multi-doc~~ ✅ / ~~timestamps~~ ✅ / ~~hex-octal~~ ✅ / ~~merge-keys~~ ✅ / complex-keys | **anchors / tags / multi-line / multi-doc / timestamps / numeric-bases / merge-keys 全部已落地** (2026-05-25 → 2026-06-01)；仅 `yaml-future-complex-keys` (`? key` 语法) 仍延后 — rare in practice | [stdlib/yaml.md](design/stdlib/yaml.md#deferred--future-work) |
| ~~FileStream~~ ✅ + ~~TextReader~~ ✅ + ~~BufferedStream~~ ✅ + async streams | **`FileStream` 已落地 2026-05-24**；**TextReader/TextWriter 已落地 2026-05-28** (`e80f0311`)；**BufferedStream 已落地 2026-05-24**；仅 **async streams**（需 L3 async）仍延后 | [stdlib/io-stream.md](design/stdlib/io-stream.md#deferred--future-work) |
| ~~Refactor CompressionStream to Stream~~ | **✅ 已落地 2026-05-24** — CompressionStream → `WrapWrite/WrapRead` 返回 `Std.IO.Stream` | [stdlib/io-stream.md](design/stdlib/io-stream.md#refactor-compression-stream-on-iostream--landed-2026-05-24) |
| ~~Refactor BinaryReader/Writer to accept Stream~~ | **✅ 已落地 2026-05-24** — `(Stream)` 构造器；byte[] 构造保留作 sugar | [stdlib/io-stream.md](design/stdlib/io-stream.md#refactor-binary-reader-stream--landed-2026-05-24) |
| libdeflate batch | 1.5× DEFLATE 快通道；bench 驱动 | [stdlib/compression.md](design/stdlib/compression.md#compression-future-libdeflate-batch) |
| Migrate existing stdlib natives to ext loader | crypto / 等可选移出 z42vm | [runtime/native-ext-loader.md](design/runtime/native-ext-loader.md#migration-of-existing-stdlib-natives) |
| ~~reader-writer-asymmetry (zbc+zpkg)~~ | ✅ 已修复 by [align-zbc-reader-writer-asymmetry](spec/archive/2026-05-27-align-zbc-reader-writer-asymmetry/) (zbc 1.7 / zpkg 0.8, 2026-05-27)；SIGS / TYPE 在 u8 TypeTag 之后加 u32 type_str_idx 作权威类型名；ReadWriteRoundTrip CI 启用 | — |
| ~~跨包 static field 初始化时机~~ | ✅ 已修复 by `dfcd1495 fix(compiler+vm): unique __static_init__ name per source file`（2026-05-15）；stdlib workaround 由 `cleanup-static-field-workarounds` spec 回收 | — |
| ~~`jit-future-safepoint-inline`~~ | ✅ landed 2026-06-03 as [inline-jit-safepoint-check](spec/archive/2026-06-03-inline-jit-safepoint-check/tasks.md) — `atomic_rmw sub + brif` 内联在 translate.rs 5 处 emit site，slow path 走 `jit_check_safepoint_slow` 新 helper | [archive/2026-05-28-jit-type-specialization/tasks.md](spec/archive/2026-05-28-jit-type-specialization/tasks.md#out-of-scope-items-deferred-for-future-spec) |
| `jit-future-f64-specialization` | F64 `fadd` / `fsub` / `fcmp` 走 native（结构与 I64 完全对称，只是 payload 类型）；等 F64-heavy benchmark 出现再做 | [archive/2026-05-28-jit-type-specialization/tasks.md](spec/archive/2026-05-28-jit-type-specialization/tasks.md#out-of-scope-items-deferred-for-future-spec) |
| TLS 后续（streaming / system-roots / keepalive-pool / server）| `add-z42-net-tls` (2026-06-03) 客户端落地后的 4 项：https `SendStreaming`、honour 系统 CA、TLS 连接池、服务端 TLS | [stdlib/net.md](design/stdlib/net.md#net-future-tls--已落地-2026-06-03-add-z42-net-tls) |
| `repl-future-decl-capture-vars` | REPL 声明的函数/类型体内引用会话变量（需注入机制；MVP 不捕获）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-decl-capture-vars) |
| `repl-future-decl-supersede` | 同名重定义 supersede（MVP 报错；需会话内符号版本化）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-decl-supersede) |
| `repl-future-tab-completion` | Tab 补全：作用域级 + `obj.`会话变量成员 + 类型名/`Type.`静态/ns 导出**已落地**（#59/#62）；余 任意 `expr.` receiver（需静态类型推断）+ 基元变量成员 + 关键字/ns 名 + LSP 客户端 | [toolchain/repl.md](design/toolchain/repl.md#repl-future-tab-completion) |
| `repl-future-syntax-highlight` | REPL 输入行 / 输出语法着色（rustyline `Highlighter` 钩子 + Lexer 分色；无前置，暂缓）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-syntax-highlight) |
| `repl-future-incremental-compilation` | Growing Transcript O(n) 重编译 → 增量模块加载（大 session 性能，benchmark 驱动）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-incremental-compilation) |
| `repl-future-load-directive` | `.load file.z42` 指令（ROI 低，MVP 不做）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-load-directive) |
| `repl-future-mobile` | mobile / WASM REPL（iOS W^X 限制，依赖 1.1.x mobile scripting）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-mobile) |
| `repl-future-debugger` | 调试集成（DAP server + VM 单步支持，0.8.x）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-debugger) |
| `exec-profile-matrix-future-aot-composition-cells` | **AOT 组合格子（`aot_pkgs≠[]`：部分/全 AOT、AOT+JIT 混合）执行 + per-zpkg 配置面**：profile 的 mode 已建成组合 `{tiers,aot_pkgs}` 能表示，矩阵占位 `skipped-not-yet`；AOT 执行（`aot.rs` stub）+ 配置（z42.toml / CLI）归 M9。落地时把 skipped 列翻 runnable，schema 结构不动 | [testing/exec-profile-matrix.md](design/testing/exec-profile-matrix.md#6-deferred) |
| `exec-profile-matrix-future-platform-bench` | **wasm/ios/android 下跑基准的 harness 编排**：profile 机制已平台就绪（探针在任意平台 VM 报真实 caps），缺各 `IPlatformBackend` 的 bench 采集；冷环境不可验 + informational 非门禁 → 待需要跨平台性能可见性时接 | [testing/exec-profile-matrix.md](design/testing/exec-profile-matrix.md#6-deferred) |
| `params-future-empty-array-codegen` | **纯 params 零实参 → 空数组作唯一实参的 codegen/VM 缺陷**：`string.Concat()`（无固定前缀形参、零可变实参）经 `_withParamsExpansion` 合成 `BoundArrayLit(0)` 作唯一静态调用实参 → 运行期崩（interp `undefined register %0` / jit `Null vs Null`）。**边界实测**：`Join("-")`（有 sep 前缀）/`new string[]{}`/normal-form 空数组直传均正常 → **非** #7 `Join`、**非**一般空数组。`migrate-stdlib-to-params` 的 Concat 新暴露；非空 params 全绿。归 `compiler`/`runtime`，待锁空闲单列 change | [changes/migrate-stdlib-to-params/proposal.md](spec/changes/migrate-stdlib-to-params/proposal.md) 「已知限制」 |
| `repl-future-eof-detection` | `Console.ReadLine()` 无法区分 EOF（Ctrl-D）与空行；`z42i` 当前仅靠 `.exit`/`.quit` 退出，待 runtime builtin 补 EOF 信号 | [toolchain/repl.md](design/toolchain/repl.md#repl-future-eof-detection) |
| `repl-future-runtime-version` | `.version` 只打印 zbc/zpkg 格式版本；z42vm 运行时版本串（profile/features/target）未经 builtin 暴露，待补（可复用 `--info` 信息面）| [toolchain/repl.md](design/toolchain/repl.md#repl-future-runtime-version) |
| ~~`ab-bench-micro`（Stage 2）~~ ✅ | 已落地 `extend-ab-bench-micro-criterion` Part A（Bencher mean/stddev + 自适应采样）+ Part B（`bench --micro-diff`：两隔离 `bench stdlib --json` 基线 + `_abVerdict`）| [changes/extend-ab-bench-micro-criterion/design.md](spec/changes/extend-ab-bench-micro-criterion/design.md) |
| ~~`ab-bench-criterion`（Stage 3）~~ ✅ | 已落地 `extend-ab-bench-micro-criterion` Part C：criterion 原生 `--save-baseline`/`--baseline` 同-runner 对照（gc_cycle_bench 纳入门禁、smoke_bench 保留不门禁，仅 src/runtime 改动时跑）| [changes/extend-ab-bench-micro-criterion/design.md](spec/changes/extend-ab-bench-micro-criterion/design.md) |
| `ab-interleave-per-run` | 逐次交错采样（比 hyperfine 双命令「base 全跑→pr 全跑」更抗 job 内漂移）；当前同机相邻已足够抵消 between-run，非必要 | [changes/add-same-runner-ab-bench-gate/design.md](spec/changes/add-same-runner-ab-bench-gate/design.md) Deferred 段 |
| ~~`ab-resample-on-suspicion`~~ ✅ 2026-09-06 | **同-runner A/B 的「可疑即复测」**已落地：只对初判 `R_lower > 1+thr` 的条目再测 k=3 轮、用**跑间比值离散度**重算区间。随之 **CI 阈值 0.25 → 0.15**、**micro tier 恢复硬门禁**、**criterion tier 降级为 informational**（0 次真阳性，且该层复测代价 +780s 不成比例）。剩余观察项：复测参数（k=3、单侧 95%）尚未在 CI 上验证跑间离散度的真实量级——离散度若偏大，症状是**真回归被放过**，那时该加 k 而不是松阈值（`ab.json` 的 `round_ratios` 为此而留）| [dev/benchmarking.md「可疑即复测」](book/src/dev/benchmarking.md) |
| ~~`retire-baseline-branch`~~ ✅ 2026-09-05 | ~~彻底删 `bench-baselines`/`bench-update.yml`~~ 已由 simplify-bench-gate 落地；剩余：e2e 死字段（`metric:"memory"`）/ `blackBox` no-op | [changes/add-same-runner-ab-bench-gate/design.md](spec/changes/add-same-runner-ab-bench-gate/design.md) Deferred 段 |

### 实施期延后（D-* 系列）

| ID | 标题 | Design doc 条目 |
|------|------|------|
| **D-2** | ISubscription chain `.AsOnce()` / `.AsWeak()` 跨 generic interface impl | [language/delegates-events.md](design/language/delegates-events.md#d-2-isubscription-chain-asonce--asweak-跨-generic-interface-impl) |
| **D-3** | N>4 arity Action / Func（自举后用 z42 写生成器）| [language/delegates-events.md](design/language/delegates-events.md#d-3-n4-arity-action--func) |
| **D-4** | 协变 / 逆变（`<in T, out R>` 等）| [language/generics.md](design/language/generics.md#d-4-协变--逆变in-t-out-r-等) |
| **D-11** | introduce-bound-visitor（review.md §2.1 visitor 抽象基类）| [compiler/compiler-architecture.md](design/compiler/compiler-architecture.md#d-11-introduce-bound-visitorreviewmd-21-visitor-抽象基类) |
| ~~`test-pipeline-future-device-run`~~ ✅ | 已实现 (2026-08-30) — `z42b-device-run` Slice 3：z42b 接管设备端 build+deploy+**实际 RUN**（wasm PR-1 / ios PR-2 / android PR-3；驱动 Playwright / xcodebuild-sim / gradle），PR-4 test-agent 从 z42b 自己 SDK 解析已装 `test` workload（删 in-tree `--agent`，dogfood workload 布局） | [archive/2026-08-30-z42b-device-run/design.md](spec/archive/2026-08-30-z42b-device-run/design.md) |
| ~~`repl-multiline-future-rbrace-floor`~~ ✅ | 已实现 (2026-08-29) — `add-repl-rbrace-floor`：`}` 自动回退一级 + 退格 floor 到前制表位。用 `Replace(WholeLine)`（唯一 redo-免疫的变量宽度删+插）+ patch rustyline `edit_insert_text` 使插入后推进光标（`[patch.crates-io]` → `z42-lang/rustyline` v14.0.0 单 commit，已同步上游）根治坑 ②「光标归位行首破坏 `} else {`」 | [archive/2026-08-29-add-repl-rbrace-floor/design.md](spec/archive/2026-08-29-add-repl-rbrace-floor/design.md) |
| ~~`compiler-future-typed-overload-resolution`~~ ✅ | 已修复 (2026-07-01) — `add-type-based-overloads`：type-based mangling（`OverloadResolver`）+ 实例方法协议豁免名单，解锁同元不同类型 ctor / method 重载 | [compiler/compiler-architecture.md](design/compiler/compiler-architecture.md#方法重载决议type-based-mangling--协议豁免名单add-type-based-overloads2026-07-01) |
| ~~`compiler-future-vcall-base-class-fallback`~~ ✅ | 已修复 (2026-05-26) — 三处协同修复：IrGen.Classes.cs `.base` 元数据用 QualifyClassName；FunctionEmitter.cs base ctor IR 名从 SemanticModel 推导；exec_vcall.rs lazy walk 对深层 base 用 ctx.try_lookup_type() | [compiler/compiler-architecture.md](design/compiler/compiler-architecture.md#compiler-future-vcall-base-class-fallback-已修复-2026-05-26) |
| `slim-terminator-future` | 装箱 `Terminator` 的 String label（per-block，非热数组，收益低于 Instruction）| [runtime/ir.md](design/runtime/ir.md#deferred--future-work) |
| `slim-instruction-stringid` (E2.P3) | `String → StringId` intern 收敛，进一步缩小 payload struct（正交 slim-instruction-enum 之后）| [runtime/ir.md](design/runtime/ir.md#deferred--future-work) |
| ~~`self-hosting-future-indexed-zpkg`~~ ✅ | 已解决 (2026-07-08) — add-indexed-zpkg-min-patch：indexed 重定义实装（zpkg 0.24，主文件 FILE 目录 + 自包含散装 zbc + hash 校验 + VM 装载）| [compiler/self-hosting.md](design/compiler/self-hosting.md#self-hosting-future-indexed-zpkg已解决-2026-07-08add-indexed-zpkg-min-patch) |
| `incremental-future-workspace-wiring` | workspace/flat 构建暂不落 cache、不 probe（WsPlan 缺 cache 目录 + gen 脚本需 `--no-incremental` 纪律）；单工程增量已落地 | [compiler/project.md](design/compiler/project.md#incremental-future-workspace-wiring) |
| `incremental-future-tsig-level-invalidation` | 文件级增量失效边取 token 保守粒度（引用文件任何变化即失效引用方，过近似只多编不错编）；「TSIG-equal 不失效」细化需 per-file TSIG 规范化 diff（剥全包自由函数泄漏段） | [compiler/project.md](design/compiler/project.md#incremental-future-tsig-level-invalidation) |
| ~~`self-hosting-future-single-vm-bootstrap-gap`~~ ✅ | 已解决 (2026-07-09) — fix-bootstrap-format-bump-deadlock：ci-bootstrap 版本差 gate + 两代自举(旧 VM 从 SDK bin/z42vm → gen1/gen2 → 新 VM;runtime/compile stdlib 分离),本地端到端验证。格式 bump CI 自动过、免手动 | [compiler/self-hosting.md](design/compiler/self-hosting.md#self-hosting-future-single-vm-bootstrap-gap已解决-2026-07-09fix-bootstrap-format-bump-deadlock) |
| `packaging-future-mobile` | mobile（ios/android/wasm）包配置化；Phase 1 只覆盖 desktop SDK + runtime | [toolchain/packaging.md](design/toolchain/packaging.md#packaging-future-mobile-mobile-包配置化) |
| `packaging-future-selector` | `packages.toml` 自动发现（组件自报 `packages=[...]`，中央零编辑）；牺牲显式 include 的可见性，apphost 数量多时再评估 | [toolchain/packaging.md](design/toolchain/packaging.md#packaging-future-selector-packagestoml-自动发现全-selector) |
| `packaging-future-artifact-naming` | `packages.toml` 的 `[package.*].artifact` 模板未被 xtask 消费，包目录命名仍是既有硬编码拼接（与模板字面值不一致）；改成真读字段会变更包目录名（外部可见），Phase 1 不做 | [toolchain/packaging.md](design/toolchain/packaging.md#packaging-future-artifact-naming-xtask-真正读-packagestoml-的-artifact-字段驱动包命名) |
| `packaging-future-byte-identical-verification` | sdk/runtime 包树端到端逐字节一致验证被并行会话的 zpkg 格式升级（minor 22→0.23）环境问题阻塞；重构逻辑本身已通过单元+部分 e2e 独立验证 | [toolchain/packaging.md](design/toolchain/packaging.md#packaging-future-byte-identical-verification-端到端字节一致验证补跑) |

### 代码内临时绕过（in-code stopgaps，待正解）

> 2026-06-01/02 修 CI 时落地的过渡手段 —— **代码里有临时绕过，正解在对应 active spec 里排期**。这两项不是 design-doc 延后，而是 `docs/spec/changes/` 下的进行中变更；列在此处供集中复查。

| 绕过点（代码） | 正解 spec | 状态 |
|------|------|------|
| `src/runtime/tests/cross_thread_smoke.rs::concurrent_gc_mode_stress_no_race_no_leak` 在 **windows `#[ignore]`**（并发 GC stale-mark race；windows-only、本地不可复现）| [spec/changes/investigate-concurrent-gc-stale-mark-race](spec/changes/investigate-concurrent-gc-stale-mark-race/) 阶段 3：loom/shuttle 验证 + 协议修复 | ⏳ 待排期 |
| ~~`src/libraries/z42.crypto/tests/ecdsa_secp256k1_vectors.z42` 的 `[Timeout]` 600s stopgap~~ | ✅ 已修复 2026-06-05 by [spec/archive/2026-06-05-optimize-ecdsa-jacobian-coords](spec/archive/2026-06-05-optimize-ecdsa-jacobian-coords/)：secp256k1 + P-256 都迁到 Jacobian 坐标（一次 ModInverse / scalar mult），round-trip 本地 ~60s → ~5.5s。`[Timeout]` 收紧到 60s | ✅ 完成 |

### 仓库结构 / 维护方向（infra，未排期）

> 战略展望（非 feature，无 design doc 条目，按 philosophy.md 归 roadmap）。来源：User 2026-06-15「这个仓库只做测试流程的」。

| 方向 | 描述 | 触发条件 |
|------|------|---------|
| `infra-slim-git-history` | **真正的克隆成本在历史**：`.git` ≈ 604 MB 而 HEAD 跟踪内容仅 ~25 MB → 历史含曾提交又删的大二进制（旧 zpkg/artifacts blob）。用 `git filter-repo` 清历史大 blob（预计降到几十 MB）+ 收紧 `.gitignore`（如 `examples/*.zbc/.zlib/.zmod` 类构建产物；注：`src/toolchain/host/examples/` 重复树连带其 466MB cargo target cruft 已于 dedup-examples 删除）。**与拆库正交,收益最大。** | clone 成本成痛点时 |
| `infra-extract-user-docs` | 本仓收敛为「核心（编译器/VM）+ 测试流程」仓；**仅外迁纯用户面 docs**（语言教程/指南/官网内容）到独立 `z42-docs`/官网仓。**留仓不外迁**（它们是开发/测试流程本体）：`examples/`（216 KB，被 C# 测试 + 打包 + zbc_compat 载重消费）、`docs/spec/`（spec-first 工作流本体）、`docs/design/`（@-included 进 CLAUDE.md）、`docs/workflow/`（build/test 命令真相源）。注意：拆当前文件到新仓**不会**缩小本仓 `.git`，须配合 `infra-slim-git-history`。 | 用户面文档成规模时 |

### 平台测试 CI / 后续（add-platform-test-pipeline 之后）

> 三平台 xtask 三阶段框架已落地（2026-06-16，wasm 端到端验证 7/7）。
> **统一执行模型（unify-test-pipeline-z42b）**：确立 z42b = 单目标执行器 / xtask = 语料编排器 /
> bundle manifest 为缝。**①（2026-08-29）**：on-device test-agent 迁入按需下载能力 workload
> `workload/test/`（取消独立 testhost 目录）。**②a（2026-08-29 PR #331）**：`z42b test <toml>`
> compile-then-test。**②b（wire-z42b-embedded-test，2026-08-29）**：`z42b test <manifest> --rid host`
> in-process 跑 bundle（共享核 `Std.Test.BundleRunner`，agent 与 z42b 共用）+ `--rid <device>` 组装
> `{app,libs,bundle}` deployable；`xtask test embedded` 委托 z42b。机制 SoT =
> [test-pipeline.md](book/src/toolchain/test-pipeline.md)。剩余：

| 方向 | 描述 | 触发 |
|------|------|------|
| ~~`package-test-workload`~~ ✅ | **Change C（已落地，2026-08-29）**：test workload 打包发布（payload-only，复用 `kind=workload-tooling` + 新 `[contents.payload]`，design D6；不进 packages.toml、无 merge）+ `workload install` 描述泛化为「平台 tooling 或能力」。CI 在 macos-arm64 单 host 建 + 归档 + 纳入 index。归档 [archive/2026-08-29-package-test-workload](spec/archive/2026-08-29-package-test-workload/) | — |
| ~~`z42b-test-take-over-device-run`~~ ✅ | **②b Slice 3（已落地，2026-08-30）**：`z42b-device-run` — z42b 接管设备端 build+deploy+**实际 RUN**（wasm PR-1 #338 / ios PR-2 #339 / android PR-3 #340；驱动 Playwright / xcodebuild-sim / gradle）+ PR-4 test-agent 从 z42b 自己 SDK 解析已装 `test` workload（删 in-tree `--agent`）。归档 [archive/2026-08-30-z42b-device-run](spec/archive/2026-08-30-z42b-device-run/) | — |
| `infra-ci-platform-test-dashboard` | CI job 跑 wasm(ubuntu+Playwright) / iOS(macos runner + Simulator `xcodebuild test`) / Android(`reactivecircus/android-emulator-runner` + KVM) 三平台 `test platform`，各产 JUnit → **GitHub Checks**（test-reporter action）聚合成 PR check runs = 跨平台测试 dashboard。GitHub 即远程同步层，无需自建服务 | 下一步（User 2026-06-16 要求）|
| `port-android-emulator-run-to-z42` | AndroidBackend.RunTests 当前桥接 `test.sh`（emulator AVD boot/poll/kill）；完整 z42 化 + JUnit 转换 | CI 稳定后 |
| `ios-simulator-test` | IosBackend.RunTests 当前 `swift test`（macOS host slice）；加 iOS Simulator `xcodebuild test -destination` 执行 + JUnit | CI 接入时 |
| `retire-platform-build-test-sh` | 三平台 z42 管线 CI-proven 后，删 `platforms/*/{build,test}.sh`（migrate-scripts-to-z42 节奏）| CI-proven 后 |
| `add-boxing-future-enum-precise` | enum 当前 I64 表示，装箱丢类型精度（GetType→Int32，`(MyEnum)o` 与 `(int)o` 不可区分）；精确 enum 装箱需 enum-as-type-entity（独立 tag/带-tag 装箱）。正文见 [`design/language/boxing.md`](design/language/boxing.md#deferred--future-work) | enum 作独立类型实体时 |
| `add-method-invoke-future-generic` | 泛型方法 `Invoke` / `MakeGenericType` / `Activator.CreateInstance<T>`，需运行期泛型实例化。正文见 [`design/language/reflection.md`](design/language/reflection.md) | 0.4.x G 流泛型实例化后 |
| `add-method-invoke-future-activator` | ~~无参 `Activator.CreateInstance(Type)`~~ ✅；~~有参构造~~ ✅ 由 `ConstructorInfo.Invoke(args)`（add-reflective-invoke）落地；~~泛型 `Activator.CreateInstance<T>`~~ ✅ add-generic-activator（0.4.3 G3）。剩带参泛型 `CreateInstance<T>(args)` + 嵌套构造泛型的方法级形参转发（`Bar<List<T>>`）| 有需求时 |
| ~~`generic-methods-future-reflective-invoke`~~ ✅ **已落地**（add-reflective-invoke，2026-08-22）| 反射式泛型方法 `MakeGenericMethod().Invoke()` + `IsGenericMethod`/`GetGenericArguments`——复用 M1 Frame `method_type_args` 载体。正文见 [`language/generic-methods.md`](book/src/language/generic-methods.md) | — |
| `generic-methods-future-type-inference` | 方法 type_args 推断——M1 要求显式 `Foo<T>(x)`；从实参推断 `T` 需类型统一 | 泛型人机工学打磨阶段 |
| `generic-methods-future-classlevel-typeof` | 类级 `typeof(T)` 具体化——当前产占位名（M1 只补方法级）；可复用 M1 物化范式（载体换 `instance.type_args`）| 有类级 `typeof(T)`→具体 的真实需求时 |
| `reflective-invoke-future-constraint-check` | 反射式 `where` 约束运行期校验——`MakeGenericMethod` 绕过编译期 `ConstraintChecker`（M1 直接调用仍校验）| serde/用户反射需运行期约束校验时 |
| `reflective-invoke-future-open-generic-cross` | 开放泛型类上的泛型方法双层交叉具化（类形参 + 方法形参同时开放）| 反射调用泛型类的泛型方法且类形参也需运行期绑定时 |
| `ctor-reflection-future-overload-resolution` | `Type.GetConstructor(Type[])` 按参数类型匹配重载——add-reflective-invoke 只做 `GetConstructors()` 枚举 + `ConstructorInfo.Invoke`；调用方用 `GetParameters()` 自选 | 需按参数类型直接取 ctor 时 |

### Backlog 项实施流程

每条 deferred 项被实施时：
1. 把对应条目从 design doc Deferred 段移入实施 spec 的"实施备注"
2. 创建 `<spec-name>` 类型的独立 spec
3. 验证 + GREEN 后归档；design doc Deferred 段移除该条目，本表索引行同步删除

---

## 已完成的实施

> 不在本文复述。每个落地特性都在 [`docs/spec/archive/YYYY-MM-DD-<spec-name>/`](spec/archive/) 下保留完整 proposal / design / specs / tasks / 实施备注。按主题或日期检索即可：
>
> - **L1 全特性**：`2026-04-04-*` 至 `2026-05-05-*`（pipeline / 工程文件 / 异常 / interface / inheritance / 参数修饰符）
> - **L2 工程支持**：`2026-04-26-*` workspace 系列、`2026-04-27-incremental-build-cache`
> - **L2 测试体系（R 系列）**：`2026-04-29-redesign-test-infra` / `2026-04-30-add-z42-test-runner` / `2026-05-05-extend-z42-test-library`
> - **L2 GC（MagrGC）**：`2026-04-29-add-magrgc-*` 系列（heap-interface / cycle-breaking-collector / drop-time-finalizer / strict-oom-rejection / external-root-scanning）
> - **L2 Interop**：`2026-04-29-impl-tier1-c-abi` / `2026-04-29-impl-tier2-rust-macros` / `2026-04-29-impl-pinned-syntax` / `2026-04-30-manifest-reader-import` / `2026-04-30-synthesize-native-class`
> - **L2 Embedding**：`2026-05-10-add-embedding-api`（H0-H3） / `2026-05-12-add-zpkg-resolver-hook`（H4 前置；platform facade 注入 zpkg 字节的 hook） / `2026-05-12-add-platform-wasm`（H4 WASM facade）/ `2026-05-12-add-platform-ios`（H4 iOS facade —— `Z42VM.xcframework` SwiftPM 包）/ `2026-05-12-add-platform-android`（H4 Android facade —— `z42vm.aar` Gradle module）
> - **L3 泛型 G1-G4**：`2026-04-22-add-generics-*` / `2026-04-23-add-generics-*` / `2026-04-24-add-static-abstract-interface`
> - **L3 闭包 / Lambda**：`2026-05-01-impl-lambda-l2` / `2026-05-01-impl-closure-l3-core` / `2026-05-02-impl-closure-l3-jit-complete`
> - **L3 Delegate / Event**：`2026-05-02-add-delegate-type` / `2026-05-02-add-multicast-action` / `2026-05-03-add-event-keyword-multicast` / `2026-05-04-add-event-keyword-singlecast` / `2026-05-04-add-multicast-exception-aggregate`
>
> 跨主题概览见 [`docs/design/`](design/) 各子目录的 `README.md` —— 每个 README 列出当前 phase 状态 + 已落地 spec 引用。
