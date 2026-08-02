# Proposal: 标准库改进——interop 收缩 + 性能

> **性质**：多阶段**总纲（charter）**。本文件确立方向 + 规则 + 相位计划；每个动代码的相位
> （A1/A2/B1–B4）**各自另起一个 change**（proposal+design+tasks，走 spec-first 确认）再实施。
> 本 change 只落**规则文档 + B0 基准前置**（docs/test 类，直接实施）。

## Why

用户提出两方面改进：① 模块划分偏乱，且希望"只有 core + 少数独立能力库随平台变，其余全平台共享"；
② 提升基础库 / 机制性能。探索后确立两条轴的方向与纪律。

### 事实校正（探索期）

用户初始设定「把全部 interop 集中到 core → 其他库才能跨平台一致」基于一个误解：

- z42c 把 `[Native]` 只降成**按名字（字符串池下标）**引用的 BuiltinInstr / CallNativeInstr，native 在
  **VM 加载期按名解析**，编码无 target/arch 信息；stdlib 源零条件编译。
- **∴ 所有 stdlib zpkg（含 core）本就跨平台字节相同**，与 interop 声明在哪无关。真正随平台变的是
  **Rust VM 二进制 + native 动态库**（如 `libz42_compression.*`），不是 zpkg。

故收缩 interop 面**不是**为"字节相同"（已成立），而是为：① 让"哪些库是平台边界、可能在某平台缺席
（如 browser 无线程）"显式化——纯脚本库 = 处处可跑的保证；② 单一、可审计的 native ABI 契约；③ 消灭重复声明。

## What Changes（本 change）

- **落规则文档**（interop 归属的两层模型 + Script-First / 接口最小化 / 单一声明点 / 性能升级阶梯）：
  `docs/design/stdlib/organization.md`（SoT 新节）+ `src/libraries/README.md §2`（摘要 + 链接）。
- **B0 性能基线前置**：加两个 e2e 基准场景（字符串重、字典重），补齐"先量测再优化"的缺口
  （现有场景只覆盖 fib/算术/启动/分发/线程，无 string/dict）。

## 规则（已确立，见 organization.md「平台边界库 vs 全平台共享库」）

1. interop 只允许在 **① z42.core**（全平台通用基础原语）+ **② 独立平台能力库**（io/net/threading/compression 等，
   承载平台相关、某平台可能缺席的能力）；**其余库纯脚本、零 interop → 全平台共享**。
2. **Script-First**：逻辑尽量脚本；interop 只提供最小基础机制。
3. **接口最小化**：interop 非必要不导出，包装薄。
4. **单一声明点**：每 native 符号全仓库只声明一次。
5. **性能升级阶梯**：脚本 → 优化机制 → 仍不达标才下沉 VM 内置。

## 相位计划（后续各自开 change）

**轴 A — 模块划分 / interop 收缩**
- A1 去重 cross-cutting 原语进 core（位转换 io.binary+ir→core；时钟 time/io/net/test→core）→ io.binary 变纯脚本
- A2 math/time 的 `__math_*` / `__time_*` 归 core，math/time 变纯脚本 wrapper（轴 A 最大一步；含 bootstrap-seed 评估）
- A3 各能力库 interop 收敛单 sink + 最小导出
- A4 编译器支撑库（ir/project/build）标注为独立 toolchain 子层
- A5（选做）分层缠绕（crypto→numerics→random）评估

**轴 B — 性能（升级阶梯序：先机制，后下沉）**
- B0（本 change）字符串重 + 字典重基准
- B1 调用路径去锁去分配（`perf-vm-iteration` Ph1，机制，interp+jit 同提速）
- B2 每对象 `Mutex<T>`（Ph2，机制，高风险）
- B3 intrinsic 表 + 去虚化（CharAt/Equals/Length→直接 opcode，机制）
- B4 native 批量内建（IndexOf/Contains/concat-N、Hex/Base64）——**仅 B1–B3 证明仍不达标时**（= 下沉，末端）
- B5 集合算法尾巴（List.Sort / Dict.Remove backfill / Clear 清槽）

## Scope（本 change 允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `docs/design/stdlib/organization.md` | MODIFY | 新增「平台边界库 vs 全平台共享库」节 + 改写 TL;DR 规则 #3 |
| `src/libraries/README.md` | MODIFY | 改写 §2 为两层模型 + 配套纪律 |
| `bench/scenarios/07_string_heavy.z42` | NEW | 字符串搜索/拼接基准 |
| `bench/scenarios/08_dict_heavy.z42` | NEW | 字典 insert/lookup 基准 |
| `bench/README.md` | MODIFY | 场景清单登记 07/08 |
| `docs/spec/changes/improve-stdlib-org-perf/*` | NEW | 本总纲 + tasks |

## Out of Scope
- 任何动 core/VM 逻辑的迁移（A1/A2/B1–B4）——各自另起 change。
- 更新 README「Extern 现状审计表」——留给 A1/A2 迁移 change 同步。

## Open Questions
- [ ] A2（math/time intrinsic 归 core）与 bootstrap-seed 的两-nightly 纪律交互，待 A2 的 design.md 评估。
