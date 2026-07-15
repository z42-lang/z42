# Design: 0.4.0 按模块整合规划

> 来源标注：`todo#N` = `docs/todo-list.md` 顶层第 N 项；`todo-11.N` = 第 11 行（0.4.0 心愿单）内第 N 子项；`FS-<ID>` = four-streams 规划的流 ID（Pc/Pv/S/L/B/G）。
> 现状标注：✅ 已落地 / 🟡 in-flight（附 change 名）/ ⬜ 待做。

## 整合原则

1. **范围以 todo-list.md 第 11 行为准**，four-streams 的 P/B/S/L/G 作为已设计好的实现细节回填到对应模块。
2. **锁模型沿用**（`parallel-development.md`）：`compiler`/`z42c` / `runtime` / `stdlib` / `toolchain` 各一把锁，`docs` 不上锁。跨锁项占多把。
3. **byte-identical 硬约束不变**：任何改 codegen/emit 的编译器项必须 C# + z42c 双侧镜像，否则打破 0.3.10 gate。
4. **子版本号弹性**：0.4.0 是一条线（0.4.0–0.4.x），由退出标准定义终点，按锁可用性排队。

## 模块架构

```
0.4.0 = 6 模块并行（按子系统锁）
┌──────────────┬──────────────┬──────────────┐
│ ① 编译器      │ ② 语法机制    │ ③ 标准库      │
│ compiler/z42c│ compiler/z42c│ stdlib       │
│ (Pc + 增量并发)│ (S 流小语法)  │ (L + z42c库入库)│
├──────────────┼──────────────┼──────────────┤
│ ④ runtime    │ ⑤ 工具链      │ ⑥ 测试·产品·文档│
│ runtime      │ toolchain    │ CI / REPL /   │
│ (Pv + 组件化) │ (z42b/bench/  │  Playground / │
│              │  publish/pkg) │  book         │
└──────────────┴──────────────┴──────────────┘
        │
   横切：G 流（泛型实例化 + 泛型反射）→ 喂 ③ 的 Deserialize<T>
```

---

## ① 编译器（`compiler`+`z42c` 锁；byte-identical 双侧镜像）

| 项 | 内容 | 来源 | 现状 | 依赖 / 备注 |
|----|------|------|------|------|
| C1 IR pass 首批 | 激活 `IrPassManager`：常量折叠 + intrinsic 折叠（`"s".Length`→3）+ DCE | FS-Pc1 | ⬜（框架已搭，零 pass）| 改 emit → 双侧镜像 |
| C2 intrinsic 表 + devirt | `(Type,Member)→{pure,const_fold,interp_op,jit_lower}` + sealed/已知类型 VCall→Call | FS-Pc2 | ⬜（设计就绪）| 喂 ④ Pv2 类型收窄，宜先于 Pv2 |
| C3 大类拆分 + BindCall D-11 | ZbcWriter / PackageCompiler.BuildTarget / TypeChecker.Calls 拆到 <200 行 | FS-Pc3 | 🟡 `split-irgen-class`（TypeChecker/Parser 已归档）| 独立 refactor commit |
| C4 compile-perf profiling | 逐阶段计时 + 已知热点（FinalizeInheritance O(N²)、QualifyClassName 重复解析）| FS-Pc4 | ⬜ | 喂 0.3.10「median ≤3× C#」gate |
| C5 增量 + 并发编译 | 多 CU collect+typecheck 并行 → 串行 IrGen/ZbcWriter；增量跳过未变 CU | FS-Pc5 + todo#4 | ⬜ | 保 ZbcWriter 确定性（common-pitfalls §1）|
| C6 build 依赖排序 | build 某项目时先编其依赖项目 | todo#1 | ⬜ | 与 workspace 编排相关 |
| C7 版本 hash 触发重编 | z42b/z42c 等嵌入版本 hash，版本不同强制重编 | todo#7 | ⬜ | 防陈旧产物；与 C5 增量协同 |

---

## ② 语法机制（`compiler`+`z42c` 锁；S 流，与 C/G 串行争锁）

| 子项 | 特性 | 来源 | 现状 |
|:--:|------|------|------|
| S1 | `params` 变长参数 | FS-S | ✅ 已落地（2026-07-01）；🟡 `migrate-stdlib-to-params` dogfood |
| S2 | `init` 访问器 + 表达式体属性 | FS-S | ⬜ |
| S3 | 索引器 `this[i]` | FS-S | ⬜ |
| S4 | 命名实参 | FS-S | ⬜ |
| S5 | `partial` class（唯一动机=拆 200 行超限大类）| FS-S / Q-A | 🟡 `add-partial-types` |

每项配 golden test + dogfood 验证。

---

## ③ 标准库（`stdlib` 锁；L 流 + z42c 库入库 + 脚本 perf，三处串行排队）

| 项 | 内容 | 来源 | 现状 | 依赖 |
|----|------|------|------|------|
| L1 模块划分整理 | stdlib 组织审计 + 重排 | FS-L1 + todo-11.8 | ⬜ | — |
| L2 JSON serde 链 | `JsonReader` 流式 → `JsonSerializer` 非泛型（`[JsonProperty]`）→ **`Deserialize<T>` 泛型（招牌）** | FS-L2 | ⬜（Invoke 已落 0.3.12）| 泛型版 ◄ 横切 G 流 |
| L3 CLI 对标 | 值校验 + 全局 flag + shell 补全 + **`--verbosity` 内置 Std.Cli** | FS-L3 + todo#3 | ⬜ | — |
| L4 z42c 基础库入 stdlib | 把编译器自用 **metadata / ir** 等数据结构抽象封装进 libraries 供复用 | todo-11.7 | ⬜ **（Q3：沿用收敛范式）** | **范式已立**：in-flight `converge-z42c-onto-z42-project`（project model→`z42.project`，后端拆 `z42c.zpkg`）；metadata/ir 是下一批候选，各自出设计 spec 定边界，勿破坏自举种子约束（bootstrap-seed.md）|
| L5 stdlib 脚本 perf 三轮 | BigInt/Coll、String/IO、JSON/YAML/TOML | FS-P6 + todo-11.7bis | ⬜ | 吃 stdlib 锁排队 |
| L6 z42-doc | doc comment → HTML/markdown + stdlib 自动发布 | FS-L4 | ⬜ | — |

---

## ④ runtime / VM（`runtime` 锁；Pv 流 + 组件化，与编译器侧并行）

**已落地基线（不重复做）**：4-slot 多态 IC、JIT I64 helper 特化、cross-zpkg OnceLock 缓存、Instruction enum 96B→32B、GC v1 三阶段。

| 项 | 内容 | 来源 | 现状 | 收益 / 阻塞 |
|----|------|------|------|------|
| R0 perf 基线刻画 | 量化已落地 IC / I64 特化 | FS-Pv0 | ⬜ | 0.4.0 起点 |
| R1 quickening + 超指令 | `FieldGet{name}`→`FieldGetAtOffset{offset}` + opcode 融合 | FS-Pv1 | ⬜ | interp 热路径 15–25%；阻塞=改写安全协议 |
| R2 JIT 直接 emit 拆箱（招牌）| Cranelift `iadd` 直发 + F64 特化 | FS-Pv2 | ⬜ | 算术循环 2–3×；**依赖 ① C2 类型信息** |
| R3 Frame 寄存器 Vec 化 | `HashMap<u32,Value>`→稠密 `Vec<Value>` | FS-Pv3 | ⬜ | 全 interp 常数因子 |
| R4 非原子 refcount | `Arc`→`Rc` feature-gate（单线程路径）| FS-Pv4 | ⬜ | string-heavy 10–15%；**profiling 门控 >5% 才做** |
| R5 devirt | IC 驱动 mono 直接函数指针 | FS-Pv5 | 🟡（部分）| call-heavy 5–10%；完整推测内联依赖 deopt → 留 0.5.x |
| R6 StringId interning | `ConstStr` `Box<str>`→`u32` | FS-Pv6 | 🟡 `optimize-zpkg-binary-layout` | 阻塞=zbc bump + 双侧镜像 |
| R7 safepoint 内联 | JIT safepoint check 内联 | — | 🟡 `inline-jit-safepoint-check` | — |
| **R8a host 统一** | host/hostrun/main 统一入口，不同平台共享简化（crates `z42-host`/`z42-hostrun`）| todo-11.4 | ⬜ **（Q2 裁决：上移 0.4.0）** | 平台差异收敛到共享抽象；先做这半 |
| **R8b 组件化骨架** | VM 组件化 cargo-feature 骨架（interp/jit/gc/corelib 可裁剪）| todo-11.4 | ⬜ **（Q2：原 0.9.5 上移）** | 只铺骨架，完整裁剪留后续；牵动嵌入接口 |

---

## ⑤ 工具链（`toolchain` 锁；B 流 + publish/package）

| 项 | 内容 | 来源 | 现状 | 依赖 |
|----|------|------|------|------|
| T1 z42b GA | z42b 作为统一工具链前端（build/test/bench...）| todo-11.3 | 🟡 `wire-z42b-host-build` | — |
| T2 z42.bench + z42b bench | 独立 `z42.bench` 包 + `z42b bench` GA + baseline 铺面 + e2e 硬门禁 + PR 自动 diff 评论 | FS-B + todo-11.6 | ⬜ | 先于 ④ perf 落地 |
| T3 publish 不依赖 desktop | `z42 publish` 复用 build 流程，不依赖 desktop workload，简化 quickstart | todo#2 | ⬜ | 见 memory `apphost-publish-needs-desktop-workload` |
| T4 workload 命令自动注册 | programs 命令行自动注册（vs spawn process，含性能/多线程对比）| todo#9 | 🟡 `add-workload-command-dispatch` | — |
| T5 xtask 路径读 z42.toml | xtask 全部路径从 z42.toml 获取，不写死拼接 | todo#8 | ⬜ | — |
| T6 package 剥离调试符号 | 发布剥离调试符号 + 符号如何下载 | todo#10 | ⬜ | 需设计符号分发方案 |
| T7 dev infra | remote command 打印 skill（远程查看）+ 定时唤醒查看 CI | todo#5,#6 | ⬜ | 开发提效，低优先 |

---

## ⑥ 测试 · 产品 · 文档

| 项 | 内容 | 来源 | 现状 | 依赖 |
|----|------|------|------|------|
| X1 tier2 平台测试流程 | **CI 当前只全测 tier1（linux/macos/windows）**；补齐 **tier2（wasm / ios / android）**：WASM(Playwright) / iOS Simulator(`xcodebuild -destination`) / Android(emulator-runner+KVM) → JUnit → GitHub Checks | todo-11.5 + FS(0.3.13)| ⬜ **（Q4：补齐 tier2，非重做）** | tier 定义见 `versions.toml [platform.*]` |
| X2 REPL | z42 原生 REPL（变量/表达式/类型声明/实例化 + 跨 line scope）| todo-11.1 | ⬜ **（Q1：上移 0.4.0，与 X3 同批）** | 前置 Semantic/TypeChecker/IR 已在自举线交付 |
| X3 Playground | z42 WASM playground | todo-11.2 | 🟡 `add-z42-wasm-playground` | 依赖 WASM VM（与 X1 tier2-wasm 协同）|
| X4 book 整理 | book 内容整理与补充 | todo-11.9 | ⬜ | 贯穿；docs 不上锁 |

### 横切：G 流泛型前置（`compiler`+`runtime`；Q5 保留）

喂 ③ L2 `Deserialize<T>` 招牌。two-step 交付：JSON 先出非泛型版保产物，G 就绪再上泛型版。

| ID | 内容 | 依赖 |
|----|------|------|
| G1 | 运行期泛型实例化 | 泛型 G1–G4 + 闭包核心（已提前落地）|
| G2 | 泛型方法 Invoke + `MakeGenericType` | G1 |
| G3 | `Activator.CreateInstance<T>` | G1 + G2 |

> 代价：违反 roadmap「不为单点提前半个 L3」，显式登记为 0.4.0 招牌前置例外；0.5.x 反射条目相应清空。

---

## 排期与锁协调

```
runtime 锁：④ R0–R8            ── 独立并行
compiler/z42c 锁：① C → G0 → ② S   ── 串行争锁（G 先于 S，serde 依赖 G）
                       ↑ 改 emit 必须双侧镜像（byte-identical gate）
stdlib 锁：③ L1 → L2 → L4 → L5   ── 串行排队
toolchain 锁：⑤ T1–T7           ── 独立并行（T2 bench 先于 ④ perf 落地）
docs 不上锁：⑥ X4 / book 贯穿
```

**关键耦合**：
1. **R2 ◄ C2**：JIT 直接 emit 拆箱需编译器 intrinsic 表提供类型收窄；C2 宜先于/同步 R2。
2. **L2 泛型版 ◄ G0**：`Deserialize<T>` 依赖运行期泛型实例化 + 泛型反射（two-step 交付：先非泛型保产物）。
3. **T2 先于 ④**：每个 perf 杠杆落地前先有 baseline，落地后硬门禁防回退。
4. **L4（z42c 库入 stdlib）牵动自举**：抽 metadata/ir 进 stdlib 会改编译器依赖面，需先出设计 spec 定边界（不可与自举种子约束冲突，见 bootstrap-seed.md）。

## Testing Strategy

- 本变更为纯规划文档，验证 = 内部引用一致性（无悬挂引用、来源标注可追溯、与 four-streams/roadmap 差异已在 proposal Open Questions 登记）。
- 各条目 spec 落地时各自带 GREEN：perf 配 bench 回归、syntax 配 golden、lib 配 [Test]、编译器改 emit 额外过双侧 byte-identical gate。
