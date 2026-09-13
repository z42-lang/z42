# Tasks: 多块 callee 内联（放宽 v1 单块限制）

> 状态：🟢 已完成 | 完成：2026-08-03 | 类型：perf（compiler，优化管线）| 创建：2026-08-03
> "全部做" 优化程序 3/4（1=cascade / 2=CSE 已合；4=LICM 待做）。扩展 add-compiler-inlining 的 IrInline。

**变更说明：** 放宽 IrInline 的「callee 必须单块」限制 → 支持含控制流（if/loop → br/br.cond + 多 Ret）
的多块 curated callee。资格：每块终结子 ∈ {Ret,Br,BrCond}（Throw 不可）+ 全 curated 指令 + 总指令数门。
展开分两 Phase：**A** 单块 callee 就地 splice（旧路径，不变）；**B** 多块 callee split+insert——拆 caller
块为 head（前半+被写形参 copy+`br entry`）与 cont（后半+原终结子），中间插 callee 各块（唯一 relabel
`__il<ctr>_`、指令 clone+remap、Br/BrCond 目标 relabel、每 Ret→绑返回值+`br cont`）。
**原因：** v1 只内联直线单块 callee；真实函数多含控制流 → 覆盖面窄。多块内联大幅扩内联面。
**文档影响：** book 优化页（内联多块展开）；z42c.semantics README（IrInline 行）。

## 独立性（D2）/ 确定性
- 仍属 `Opt.Inline`；单独开正确。Phase B 唯一标签前缀按处理序递增 → 确定 → 自举不动点收敛。

## 安全边界
- callee 体全 curated（无 call/alloc/副作用）→ 内联块无嵌套调用 → 无间接递归、Phase B 终止。
- 被写形参 copy 放 head（只执行一次 → loop 安全）；只读形参直代入实参。
- 仍排除：异常表 / varargs / VCall / 跨包 / Throw 终结子。

- [x] 1.1 `InlineState`（游标/预算/标签计数/RegTypes 记录/changed 共享）
- [x] 1.2 `_eligibleCallee` 放宽多块（每块终结子+curated 检查，总指令数门）+ `_termInlinable`
- [x] 1.3 Phase A `_inlineSingleBlock`（旧单块 splice）+ Phase B `_inlineMultiBlock`/`_spliceMultiBlock`/`_cloneCalleeBlock`
- [x] 1.4 `_writtenParamsAll`（跨块）/ `_record` / `_totalInstrs`
- [x] 1.5 单测：多块 callee（abs 含 if）内联（call 消失 + neg + br.cond 内联进来）
- [x] 1.6 `xtask test` 全绿 + self-host 不动点（引入当次 gen1≠gen2 破一代=D7，重建自愈）
- [x] 1.7 文档同步（book + README）
