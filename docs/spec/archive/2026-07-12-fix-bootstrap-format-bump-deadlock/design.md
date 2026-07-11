# Design: 两代自举根治格式-bump CI 死结

## Architecture

```
ci-bootstrap（改造后）
  [0/5] cargo build z42vm(new)              ← 新 minor reader（不变）
  [1/5] download nightly SDK → seed          ← 旧 z42c + 旧 stdlib + 旧 bin/z42vm（不变）
  [1.5] 版本差检测:seedMinor vs currentWriterMinor
        ├─ 相等(日常)→ 走现有 [2/5] 单 VM 快路径(零改动)
        └─ 不等(格式 bump)→ 两代自举:
             Gen1: 旧VM(seed bin/z42vm) + 旧z42c → 编当前 z42c源+stdlib源
                   → gen1 产物(旧格式外壳/新逻辑),Z42_LIBS=旧stdlib
             Gen2: 旧VM + gen1 z42c → 编当前 z42c源+stdlib源
                   → gen2 产物(新格式),Z42_LIBS=gen1产的stdlib
             切换: 把 gen2 z42c + gen2 stdlib 落到标准 artifact 位置
  [2/5..5/5] 新VM(new) 消费 gen2 产物 → build xtask / test / package（不变）
```

## Decisions

### D1: 用 SDK 自带的旧 VM,而非 cargo 新 VM,跑种子阶段
**问题**:根因是新 VM(strict-pin 到新 minor)读不了旧种子。
**决定**:种子阶段(Gen1/Gen2)一律用 nightly SDK 里的 `bin/z42vm`(旧 minor,能读旧种子)。
新 VM 只在 Gen2 产物就绪后接管。**前提已满足**:每个 RID 的 nightly SDK 都发 `bin/z42vm`
(实测 linux `./bin/z42vm`、windows `bin/z42vm.exe` 均在)。旧 VM 是我们自己上一版的原生
产物、跑在同一 RID,可信可运行。

### D2: 版本差 gate——日常 run 零成本
**问题**:两代 = 多两遍全量编译,不该让每次 CI 都付。
**决定**:比较种子 zpkg minor(读 `programs/z42c/z42c.driver.zpkg` header 第 6-7 字节)与当前
`ZpkgWriterZ.Minor`(源码常量)。相等 → 现有单 VM 快路径,零改动零成本;不等 → 两代。
绝大多数 push 无格式 bump → 走快路径。

### D3: 为什么 Gen2 不可省
Gen1 z42c 由**旧 z42c**(写旧格式)编出 → gen1 的**外壳是旧格式**(新 VM 读不了),尽管其
内部逻辑=当前源=会写新格式。必须再跑一代:旧 VM 跑 gen1 z42c → gen1 逻辑写新格式 → gen2
外壳是新格式 → 新 VM 能读。stdlib 同理:gen1 stdlib 旧格式(供 gen1 z42c 依赖),gen2 stdlib
新格式(供新 VM)。

### D4: 两代编排的落点——xtask 子命令 vs 纯 bash（Open Question,倾向 xtask）
**问题**:逻辑放 action.yml 的 bash,还是抽 `xtask bootstrap-twogen`?
**权衡**:
- xtask 子命令:可维护(z42 写、有类型)、可本地 mock 干跑(造旧种子测编排)、复用
  `_compilerMembers`/`WorkspaceBuild` 拓扑。**但**鸡蛋:跑 xtask 需先有能跑的 z42c——而两代
  正是为了产出它。→ 仍需一小段 bash 先用旧 VM+旧 z42c 编出 xtask(Gen1 的一部分),再由
  xtask 接管 Gen1 剩余 + Gen2。
- 纯 bash:自包含,但两 VM×两代×stdlib+z42c 的编排在 bash 里易错、难读。
**倾向**:混合——bash 只做"旧 VM 编出 xtask"最小引导,其余 gen 编排进 xtask。**实施期先做
纯 bash 打通 CI(能绿最重要),绿后再评估抽 xtask**(避免一上来就双重未知)。

### D5: 依赖 support-先行纪律(正交前提,不改)
两代自举要求"旧 nightly 的 z42c 能编当前源"(语法/stdlib-API 维度)。这条纪律
(bootstrap-seed.md + `xtask test bootstrap` 边界检查)**已在强制**。两代自举只解决**格式**
维度的坎(新 VM 读不了旧种子),不碰语法/API 维度。两者合起来 → 格式 bump 全自动、无手动。
**验证**:本次 indexed(0.24)只改格式、没用新语法/API,旧 z42c 能编当前源 → 两代自举本可
让它自动过。

### D6: major bump 与首次落地
minor bump 走两代。major bump(改 magic/header layout,迄今未发生)可能连旧 VM 读种子都不行
——但那种断裂本就需人工,不在自动化范围(design 标注,不实现)。
**首次落地无鸡蛋**:当前已发布的 nightly(本次手动修的 0.24)都带 bin/z42vm,两代自举首跑即有
可用旧 VM。

## Implementation Notes

- 版本差读取:`xxd -s6 -l2` 或 bash 读 header 字节;与 `grep ZpkgWriterZ.Minor` 源码常量比。
- Gen1/Gen2 的 `Z42_LIBS` 切换是关键易错点(gen1 用旧 stdlib,gen2 用 gen1 产的 stdlib)——
  design D3 已定,实施加显式注释 + 每代后校验产物 minor(gen1 应旧、gen2 应新)。
- 产物落位:gen2 的 z42c 七包 + stdlib flat dist 落到 `artifacts/build/compiler/...` +
  `artifacts/build/libraries/dist/release`(现有 [2/5] 之后步骤期望的标准位置)。
- 保留现有失败信息 + 每代耗时日志,便于 CI 上调试(本地不可完整验)。

### D7: 验证过的精确 recipe + runtime/compile stdlib 分离（本地端到端跑通 2026-07-09）

> ⚠️ **原两代草图(D1-D3)漏了一个会实现错的关键**:z42c 依赖 stdlib,而"用旧 VM 跑
> 新逻辑 z42c 编 z42c(需新 stdlib 的 TSIG)"时,旧 VM 又得**加载**新 stdlib 来运行 z42c
> ——旧 VM strict-pin 读不了新 stdlib,看似死锁。**解法**:z42c 的运行时 stdlib 与编译期
> stdlib 走**不同来源**——z42vm dep 搜索序 = [entry-zpkg 目录, Z42_LIBS],把 gen1 z42c 放在
> **同目录带旧 stdlib**(旧 VM 运行时加载 ✓),而 `Z42_LIBS` 指**新 stdlib**(gen1 z42c 的
> 新 ZpkgReader 编译期解析 TSIG ✓)。两者分离,死锁解除。

**已用真实紧邻种子(0.24 nightly)+ 人造 0.25 bump 在本地端到端跑通的命令序列**(build
一律 per-member `build --workspace --release`,**不用 flat `--output-dir`**——后者破坏编译器
兄弟包类型解析报 E0402,已知坑):

```
seed = 0.24 种子 z42c 7 包 + 0.24 stdlib(flat 目录 $SEED)；oldVM = SDK bin/z42vm(0.24)

# Gen1 z42c: 旧VM + 旧种子 z42c → 编当前源 → gen1(旧壳/新逻辑)
cd src/compiler; Z42_LIBS=$SEED  $oldVM $SEED/z42c.driver.zpkg -- build --workspace --release
  → artifacts/build/compiler/*  = gen1 z42c(0.24 壳, 0.25 逻辑)   [种子写旧壳; runtime+compile 均旧 stdlib,自洽]

# Gen1 stdlib: 旧VM + gen1 z42c(entry-dir 带旧 stdlib 供运行时)→ 产新 stdlib
G1RUN = gen1 driver+6兄弟 + 0.24 stdlib(同目录)
cd src/libraries; Z42_LIBS=$G1RUN  $oldVM $G1RUN/z42c.driver.zpkg -- build --workspace --release
  → artifacts/build/libraries/* = 0.25 stdlib   [gen1 逻辑写新; stdlib 自包含,兄弟走 per-member dist]

# Gen2 z42c: 旧VM + gen1 z42c(entry-dir=旧 stdlib 运行时) + Z42_LIBS=新 stdlib(编译期 TSIG)
FLAT25 = 0.25 stdlib flat 视图
cd src/compiler; Z42_LIBS=$FLAT25  $oldVM $G1RUN/z42c.driver.zpkg -- build --workspace --release
  → artifacts/build/compiler/*  = gen2 z42c(0.25 壳)   [topo 序:兄弟在本轮先被重写为 0.25 再被依赖]

# 切换: 新VM(cargo, 0.25) + gen2 z42c + 0.25 stdlib → 正常 [2/5..5/5]
Z42_LIBS=$FLAT25  $newVM artifacts/build/compiler/z42c.driver/release/dist/z42c.driver.zpkg -- build scripts/xtask.z42.toml ...
  → 0.25 xtask.zpkg,新 VM 跑通 ✓
```

要点:①gen1 建 z42c 时种子/stdlib 全旧,自洽;②gen1 建 stdlib 时 stdlib 自包含,不需外部
stdlib,故 runtime(entry-dir 旧)与 compile(内部兄弟新)不冲突;③gen2 建 z42c 是唯一需要
"运行时旧 stdlib + 编译期新 stdlib"分离的步骤——靠 entry-dir vs Z42_LIBS 分离达成;④per-member
topo 序保证兄弟包在被依赖前已重写为新格式。

## Testing Strategy

- **本地(有限)**:造一个"旧种子"(把当前 0.24 SDK 的 zpkg 降级模拟成 N-1?不可行——需真旧
  writer)。更现实:留一份**真实旧 minor 种子**(如手动存的 0.23 SDK)+ 旧 VM,本地干跑两代
  编排,断言 gen2 产物 minor=当前、且新 VM 能跑。这能测编排逻辑,但不能测真实 CI download 路径。
- **CI(权威)**:本 change 合入后,**下一次真实格式 bump** 才是终极验证。为降风险,可先在一个
  **人造 bump 分支**(临时 +1 minor)上跑 CI,观察两代自举是否自动过、publish-nightly 是否发出
  新种子 → 确认闭环,再撤销人造 bump。
- 非 bump 回归:确认版本相等时走快路径、行为与现状逐字节一致(CI 全绿不回归)。
