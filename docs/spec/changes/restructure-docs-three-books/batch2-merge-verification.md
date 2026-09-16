# 批 2 的 7 组合并：字段级核实结果

> 沿用批 1 的教训——**「grep 到标识符」不等于「描述还准」，核实要落到字段/函数这一级**。
> 结论：`migration-manifest.md` 对其中 **2 组的主干判断是反的**，按证据改。

## 清单原判 vs 实测

| 组 | 清单原判 | 实测 | 处置 |
|---|---|---|---|
| **gc** | book 三页主干，design/gc.md(1090) 只贡献「接口形状 / Phase / 权衡」 | ❌ **反了**。book 三页讲的是**调参旋钮 / nursery / 增长闸门 / TLAB / SATB**；`design/gc.md` 的 **Safepoint 协议占 706 行**（55–761），逐节都是带 change 名的实现机制，book 完全没覆盖 | **design/gc.md 成为 GC 机制主干页**，book 三页作为专题页并列 |
| **native-ext** | book `native-extensions`(182) 主干，design(253) 只贡献「新增 ext lib 配方 + 平台差异」 | ❌ **反了**。design 版 253 行有完整架构（Why not BUILTINS / Architecture / Pieces / Platform variance / Adding / Tier-1 关系 / Migration），比 book 版宽 | **design 版作主干**，吸收 book 版的两个范式实例 |
| safepoint | design 自称未实施 | ✅ 属实（`> 状态：DESIGN（GC safepoint 已实施，泛化未实施）`） | design 贡献「泛化」设计，并入 |
| diagnostics | design 未实施 | ✅ 属实（`> 状态：DESIGN（部分已实施，扩展未实施）`） | 并入 |
| load-context | design 部分落地 | ✅ 属实（`> Phase 1 地基 + Phase 2 惰性卸载已落地；强制清理 / 诊断仍 DESIGN`） | 并入未落地部分 |
| ir-specialization | design 未实施 | ✅ 属实（`> 状态：DESIGN（目标架构，未实施）`） | 并入 |
| jit | design 的「模块加载时预热式 JIT」已被 lazy per-function 取代 | ✅ 属实 | 仅留 Cranelift 后端图 |

## gc 组的核实证据（推翻清单的依据）

`design/gc.md` 55–761 行「Safepoint 协议」下的 11 个子节，**逐个在当前 Rust VM 里命中**：

| 子节 | 当前 `src/runtime/src/` 命中 |
|---|---|
| GC mode selection | 37 文件 |
| Write barrier contract | 26 文件 |
| Debug invariants | 73 文件 |
| Pause histogram | 17 文件 |
| Heap snapshot export | 74 文件 |
| Finalizer contract | 30 文件 |
| safepoint / park / NativeParkGuard | 63 / 54 / 20 文件 |

且 `MagrGC` trait 本身仍在（`src/runtime/src/gc/heap.rs:55`），`ArcMagrGC` 亦在。
⇒ **这是一份活文档**，不是历史残留。

> 反证（说明核实有效）：同一批里 `stop_the_world` / `stw` **0 命中** —— 那部分确实已不存在。

## 教训

清单是**基于文件名与篇幅**做的判断（「book 更新、design 更老」）。实际决定主干的是
**覆盖面**：谁把机制讲全了谁是主干，与新旧、长短无关。后面几批（3/4/5）的
「主干判定」列同样**不可直接照搬**，都要落到字段级核实。
