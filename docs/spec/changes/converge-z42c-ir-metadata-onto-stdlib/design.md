# Design: z42.ir / z42.metadata 收敛

> 配套 [proposal.md](proposal.md)。记录实施决策与分阶段 + 自举验证。

## 决策 1：一个库 z42.ir（User 定，合并两半）

IR 模型 + zbc 格式 + zpkg 后端（读/写/构建 + TSIG + PackageTypes）+ 类型导出/依赖索引，全进单库
`z42.ir`。deps = z42.core + z42.encoding + z42.io + z42.crypto。

合并理由（User 裁决）：zbc（单模块字节码）与 zpkg（包/多模块）单向耦合（zpkg→zbc），REPL 两半都要；
一个库 = z42c 只加一条 dep、一次拓扑、一个 GREEN gate，最简。轻量消费者背 crypto/io 的代价可忽略
（谁用 IR 基本都要读包）。namespace 三段（Z42.IR / Z42.IR.BinaryFormat / Z42.Project）在一个库里共存
——库可含多 namespace，无碍。

## 决策 2：namespace 保持不变（MOVE，非并存）

converge-z42c-onto-z42-project 期 `z42.project` 用了**新** namespace（`Z42.Build.Project`）因它与旧
`z42c.project`（`Z42.Project`）**并存过渡**。本 change 是**纯 MOVE**（删编译器副本、同一份进 stdlib，
无并存），故 `Z42.IR` / `Z42.IR.BinaryFormat` / `Z42.Project`（后端现名）**保持**——好处：所有
`using Z42.IR;` 调用点**一字不改**，只有 `.z42.toml` 的 deps 换名。churn 最小、字节漂移面最小。

> flat-libs 同名 first-wins 串味风险（converge 的核心顾虑）**不适用**：串味需**两份**同名文件同时在
> flat libs；MOVE 后编译器副本已删，全局只一份 → 无串味。（converge 当年要改 namespace 是因清单模型
> 两份真并存过。）

## 决策 3：CacheStore 留构建侧（User 定）→ 迁 z42c.pipeline

CacheStore（增量构建缓存，source-hash → 跳过重编）是**构建工具策略、非格式契约**，不入 z42.ir。
消费者 = z42c.driver（IndexedDist/IncrementalDriver）+ z42c.pipeline（IncrementalBuild）。删 z42c.project
时把 `CacheStore.z42` **迁入 z42c.pipeline**（driver 已 dep pipeline）；保持 namespace `Z42.Project`
→ 消费者 `using Z42.Project;` 零改。REPL 不用它，零影响。

## 决策 4：拓扑与依赖

```
z42.core ─┬─► z42.encoding ─┐
          ├─► z42.io ────────┼─► z42.ir ─► z42c.semantics ─► z42c.pipeline ─► z42c.driver
          └─► z42.crypto ────┘
```

- z42.ir 先于全部 z42c.*。
- libraries workspace `default-members` 加 z42.ir（在 z42.encoding/io/crypto 之后）。
- compiler workspace 删 z42c.ir + z42c.project member。
- z42c.{semantics,pipeline,driver} deps：去 `z42c.ir` + `z42c.project`，加 `z42.ir`；CacheStore 迁 z42c.pipeline。

## 决策 5：分阶段 + 自举验证（关键——不可一把梭）

单库 → 三阶段（每阶段独立 GREEN + 不动点 7/7）：

**阶段 A —— 落 z42.ir（并存自测，不进 z42c libs）**
1. `src/libraries/z42.ir/`：拷 z42c.ir 源 + z42c.project 后端 6 文件（不含 CacheStore）+ toml + [Test]。
2. 进 libraries workspace default-members，产 `z42.ir.zpkg`；round-trip [Test] 绿。
3. **不**加入 z42c 构建 libs（防并存串味）。

> ⚠️ 并存期串味：z42.ir 与 z42c.ir / z42c.project 同 namespace 同文件名同在 flat libs → **会**串味。
> 故阶段 A 的 z42.ir 仅自建自测；阶段 B 删旧包的**同一提交**里切 z42c 依赖 → 任一时刻只一份。
> （同 bootstrap-seed.md「删+供种同一原子变更」纪律。）

**阶段 B —— z42c 切 z42.ir + 删 z42c.ir/z42c.project + 迁 CacheStore（同一原子提交）**
1. z42c.{semantics,pipeline,driver} deps：去 `z42c.ir` + `z42c.project`，加 `z42.ir`。
2. `CacheStore.z42` 迁 `src/compiler/z42c.pipeline/src/`（namespace 不变）。
3. **删** `src/compiler/z42c.ir/` + `src/compiler/z42c.project/` + compiler workspace 两 member。
4. **验证**：self-host 7/7 byte-identical + test compiler 全绿。

**阶段 C —— CI 拓扑 + 文档**
1. ci.yml：stdlib 构建纳入 z42.ir（拓扑序在 z42c.* 前），bootstrap 路径核对。
2. 文档：compiler-architecture（IR/zpkg 归属）、project.md、doc-system 索引、
   converge-z42c-onto-z42-project/design 决策 1 更正（后端下沉 z42.ir，作废 z42c.zpkg）。

## 决策 6：seed 轴（bootstrap-seed.md）

z42c 源开始 `using` z42.ir 的 API：z42.ir 是**新库**，其 API 必须先随一个 nightly 发布，z42c 源才能用
——但本 change 里 z42.ir 的内容**就是** z42c.ir 原样搬，API 面零新增（同类同方法）。故种子 z42c
（不含 z42.ir 但**自带** z42c.ir 逻辑）编当前源时：当前源引用 `z42.ir` 的类 = 种子 stdlib 里必须有
`z42.ir.zpkg`。**CI ci-bootstrap 先建 stdlib（含 z42.ir）再编 z42c** → 满足。本地同理（先 build stdlib）。
无「用了比上一 nightly 更新语法/格式」越界（纯包重组，无新语法、无格式 bump）→ `xtask test bootstrap`
应直接过。

## 风险与回退

- **主风险**：并存期串味（阶段 A/C）。缓解：新库不进 z42c 的 Z42_LIBS，删+切同一提交。
- **回退**：每阶段独立提交，红则回退单阶段。字节不动点是硬 gate——不过不 commit。
- **本地可验**：warm 路径（种子 = 下载的 0.33 nightly）全程本地跑；cold/CI 拓扑以 CI 为准。
