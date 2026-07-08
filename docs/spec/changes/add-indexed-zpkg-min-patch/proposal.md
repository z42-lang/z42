# Proposal: indexed zpkg 重设计——最小 patch 分发（DRAFT，不持锁）

> 🔴 DRAFT。前置：`add-file-level-incremental`（cache SoT）落地。涉及 zpkg 格式 bump +
> VM reader，届时占 `compiler` + `runtime` 锁，并受分阶段引入纪律（support 先行、
> 晚一个 nightly 再 use，见 bootstrap-seed.md）约束。

## Why

1. User 需求（2026-07-05/07 裁定）：**用户更新 patch 最小**——indexed 布局下，改一个
   源文件后 dist 里**只有主 zpkg（索引/目录，允许变）+ 对应文件的 zbc 变化**，其余
   未改动文件的散装 zbc **逐字节不动**。
2. 旧（C# 时代）indexed 设计做不到：散装 zbc 是 stripped 形态（仅 BSTR/FUNC），签名
   引用主文件**全局 SIGS/STRS 池**——单文件变更扰动全局池，可能连带其它 zbc 的池索引
   漂移。z42c 自举重写也从未实现该模式（`self-hosting-future-indexed-zpkg`，VM reader
   对 indexed 显式 bail）。

## What Changes（方向草案）

1. **散装 zbc = 自包含 fullMode**（与 cache 条目同形态）：每文件 zbc 只依赖自身源 +
   跨文件引用按 FQ 名编码（泛型为代码共享无跨文件单态化拷贝）→ 未失效文件的 zbc
   天然字节稳定。dist 的 indexed 输出 ≈ 把 cache 条目投影进 `<dist>/<rel>.zbc` +
   只重写失效条目。
2. **主 zpkg 退化为目录**：文件清单（rel 路径 + hash + ns）+ 包级 META/DEPS/entry +
   （评估）聚合 TSIG。主文件每次变更重写（User 已确认允许）。
3. **VM reader**：z42vm 实现 indexed 加载（按清单逐 zbc 读入，fullMode 单文件即完整
   IrModule）；zpkg minor bump + strict-pin 同步（version-bumping checklist 全套）。
4. **patch 面**：更新分发 = 主 zpkg + 变更 zbc 子集；`indexed-minimal` fixture 用
   z42c 重生（解除 minor=22 旧基线搁浅）。

## Out of Scope（届时细化）

- 增量判定/依赖失效逻辑（由前置 change 提供，本 change 只做投影 + 格式 + VM 消费）。
- 分发工具链（差量打包/签名校验）——另行评估。

## Open Questions

- [ ] 聚合 TSIG 放主文件 vs 留散装 zbc 的 EXPT（跨包消费方 DepScan 读哪边）。
- [ ] debug（pack=false 默认 indexed）与 release packed 的组合矩阵是否维持 project.md 现表。
- [ ] `.zsym` sidecar 在 indexed 下的形态（per-file vs 包级）。
