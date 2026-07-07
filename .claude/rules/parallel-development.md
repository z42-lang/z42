# 并行开发：子系统互斥锁

> 触发条件：**两个及以上 in-flight change 同时实施**时（无论并行 agent / 多人 / 单人来回切）。
> 流程主线见 [workflow.md](workflow.md)；本文件补齐其缺失的**并行维度**，只一条核心约定。

---

## 核心约定（一句话）

**每个 change 开工前声明它动哪些子系统；一个子系统同一时刻只允许一个 in-flight change。** 跨子系统的 change 要同时占用所有相关子系统，**全部空闲才能开**，否则排队（等占用方归档）。

为什么够用：名义文件互斥的流，实践中总会汇聚到少数共享基础设施文件（如 `PackageCompiler` / `WorkspaceBuildOrchestrator`），文件级检测又会因 Scope 漂移失准。子系统级粗锁用"一眼能判"的代价，把这两类返工一次堵死。

---

## 子系统划分

沿用 `test changed` 的子系统划分心智模型（`runtime|compiler|stdlib`——按改动目录挑 stage），补两个：

| 子系统 | 范围 |
|--------|------|
| `compiler` | `src/compiler/`（z42c 自举编译器源码，用 z42 写） |
| `runtime` | `src/runtime/`（Rust VM：interp / jit / aot / gc） |
| `stdlib` | `src/libraries/`（.z42 标准库） |
| `toolchain` | `src/toolchain/` + xtask dispatch |
| `docs` | `docs/` —— **不进互斥锁**，见下 |

> 划分若有增减（新增顶层目录等），同步更新本表 + `docs/spec/changes/ACTIVE.md` 表头。

---

## 三条配套规则

1. **跨子系统 change 占用全部相关锁**
   一个 change 若动 `compiler` + `runtime`（如 zbc/zpkg 格式变更要同时改 z42c writer 和 Rust reader），就同时占两把锁；任一被占 → 整个 change 排队。这正是"隔离流偷改共享基础设施"被自动拦下的地方。

2. **docs 不上锁**
   `docs/` 人人要碰，整体上锁会把所有 change 串死。docs 沿用 [workflow.md 阶段 3](workflow.md) 已有细则处理：**两个 change 改同一 markdown 不同段 = 冲突 → 串行**；改不同文档 → 不冲突。

3. **粒度停在子系统级，不做文件级例外**
   两个 `compiler` change 哪怕文件不重叠也串行。这是本约定"简单"的代价，刻意接受——不开"Scope 可证明不相交就并行"的口子（那会把文件级聚合复杂度加回来）。

---

## 这个约定的代价（认清）

**它会过度串行化同一热子系统内的改动，哪怕两个 change 文件其实不重叠。** 接受它，因为：

- 同子系统内"看似不重叠实则微妙耦合"恰恰是返工高发区（尤其 `runtime` 的 GC/JIT/safepoint 边界），粗锁在这里是优点不是缺点。
- roadmap 想要的 **A‖B‖C 并行依然成立**：A=`stdlib`、B=`compiler`、C=`runtime` 是不同子系统，照样放行。本约定只串行"同子系统"的活，不碰真正想要的跨流并行。

---

## ACTIVE.md 账本

**位置**：`docs/spec/changes/ACTIVE.md`（与 change 目录并列，扫描活跃占用一眼可见）。
**内容**：一张「子系统 → 当前持有 change」小表（格式见该文件自身）。

**维护时机**（挂接 workflow.md 阶段）：

| workflow 阶段 | ACTIVE.md 动作 |
|--------------|---------------|
| 阶段 2 创建 change 容器 | 声明本 change 占用的子系统；逐个查 ACTIVE.md → 任一被占则**停下排队**，空闲则登记为持有者 |
| 阶段 7 实施中扩张到新子系统 | 立即停（沿用越界防护）→ 查该子系统是否空闲 → 占用方再继续 |
| 阶段 9 归档 | 释放本 change 持有的全部子系统行 |

---

## 与其他规则的关系

- **workflow.md 阶段 3 冲突表**：留作 docs/markdown 的细则处理；src 代码的并行判定上移到本文件的子系统锁。
- **philosophy.md 根因修复**：某子系统反复成为锁争用点 → 说明它太大/耦合太多，按根因修复评估拆分（独立 refactor change）。
- **越界防护（workflow.md）**：阶段 7 扩张到新子系统时，先过本约定再继续。
