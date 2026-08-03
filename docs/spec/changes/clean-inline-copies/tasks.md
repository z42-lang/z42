# Tasks: 清理内联代码（内联只读实参直代入）

> 状态：🟡 进行中 | 类型：perf（compiler，优化管线）| 创建：2026-08-03
> 依赖：add-compiler-inlining（IrInline / OptSet）。同分支延续（PR #100 线）。

**变更说明：** 内联展开时，对 callee **只读形参**（body 未写）直接用调用方实参寄存器代入，
不再 emit `copy paramReg, arg`；并让 const-fold 穿透单赋值 copy（copy-of-const → dst 也视为常量）。
**原因：** v1 内联对每个形参 emit 一条 `copy`，给 interp 增回 per-arg dispatch，抵消部分内联收益、
且留冗余 IR。直代入从源头免去这些 copy；const-穿透让常量实参的内联算术进一步折成常量。
**文档影响：** `docs/book/src/runtime/optimization-pipeline.md`（内联展开①形参绑定改述 + const-穿透）；
z42c.semantics README（IrInline 行）。

## 独立性（design D2 沿用）
- 直代入是内联展开内部改动，`Opt.Inline` 单独开仍正确（不依赖清理 pass）。
- const-fold 穿透 copy 属 `Opt.ConstFold` 增强，单独开仍正确。

- [ ] 1.1 `IrInline`：`_writtenParams` 判 callee 形参是否被写；只读形参 body 中直接 remap 到 `arg[p]`，
      不 emit copy；被写形参保留 `copy (p+offset)=arg[p]` + remap 到 `p+offset`（InlineCtx 承载 remap 上下文）
- [x] 1.2 ~~const-fold 穿透 copy~~ **不需要**：直代入让内联体算术直接引用实参寄存器，常量实参
      本就被现有 const-fold 直接折叠（`Add(2,3)`→`const 5`）；const-穿透-copy 是**非内联** copy 场景的
      独立增强，本 change 不含（避免给 const-fold 增风险）
- [ ] 1.3 内联单测更新/新增：直代入后 IR 形态（无 param copy）、const 实参内联折成常量、独立性
- [ ] 1.4 `xtask test` 全绿 + self-host 不动点。**注（D7）**：改内联变换（param-copy→直代入）当次
      gen1≠gen2 破一代（首建 2/5：z42c.semantics/pipeline 因内联输出变了）；in-tree z42c 变直代入后
      重建即 gen2==gen3 自愈 → 收敛 5/5。直代入确定性（writtenParams+实参代入，无 hash/指针非确定源）。
- [x] 1.5 A/B 复测（heavy.z42，interp best-of-3）：直代入 vs param-copy——**zpkg 开销减半**
      （over off 基线 +95B→+47B，即 1123→1075B），内联体更快（~278→232ms），仍 ~3.3× vs 关内联；输出不变。
- [ ] 1.6 文档同步（book 优化页 + README）
