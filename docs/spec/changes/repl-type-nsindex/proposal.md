# Proposal: REPL type→namespace 符号索引（去掉 completer 的「全扫活跃 ns」）

> 承接 `repl-per-symbol-reconcile`（PR #98 T2 completer）的后续 perf。硬约束延续：**stdlib zpkg 保持
> packed 单文件、最小化**——本 change **不动 zpkg**，只扩展本地可再生缓存 `.z42-nsindex`（+~5KB）。

## 问题

T2 completer（首次引用某类型时 per-type reconcile）已把首次 `Console` 从 1.7s 降到 0.33s。但相位计时
（单调时钟逐段打点）显示：首次符号 eval 的**瓶颈是 per-type reconcile 本身（99–231ms）**，两次编译各仅
~15–30ms。再逐命名空间打点发现：completer 对**每个**活跃命名空间（usings + prelude Std/Std.Runtime）都读
一次模块找候选类型，而一个类型只存在于其中**一个** ns——其余全是白读（首次 Console 白读 ~45ms/6 个 ns，
Math 白读更多，因 Math 在第 4 个 using）。根因：`.z42-nsindex` 只记「包→命名空间」，completer 不知道
`Console` 在哪个 ns，只能全扫。

> 一个被实测**证伪**的假设（记录以免重走）：曾推测 void 调用（`Console.WriteLine`）在表达式优先路径下
> 「不能返回 void」的类型错会击穿 per-type、触发整包回退 `_loadUsingsPackages`。加单调时钟打点后实测
> per-type 后 `errs=0`（表达式形式直接编过），整包回退**根本没触发**。据此写的「E0401 门控」修复对所有
> 实测场景都是 no-op，已回退。**教训**：性能假设先量测再改。

## 方案（借鉴 .NET/rustc/javac 的符号元数据索引）

把 ns 索引升级 `NSIDX1→NSIDX2`：每 ns 字段由 `ns` 变 `ns=T1,T2`（该 ns 声明的类型短名，逗号连接）。
completer 经 `DepScanResult.NsMayHaveCandidate` **只读「索引显示声明了某候选类型」的活跃 ns**。

- **cold-start（cache-miss）**：open-all 顺带 `ReadModuleTypes` 提取每 ns 类型短名（一次性 ~159ms/490
  类型/26 包，落盘）。
- **warm（cache-hit）**：`_scanFromIndex` 从落盘 `NsTypes` 直接建 `TypeShort/TypeNs` 映射，不重读模块。
- **索引空（旧缓存/写失败/非惰性 `ScanDirs` 路径）**：`NsMayHaveCandidate` 恒 true → 退化为全扫（旧行为，
  正确性不变）。

## 效果 / 约束

- Math.Max **0.26→0.18（-31%）**、Console 0.34→0.29、List 0.38→0.34、Dict 0.40→0.36。剩余下限 = 那一个
  **必要**的 reconcile（List ~150ms，索引消不掉）。
- **零 zpkg 格式改动**（只动 `.z42-nsindex` 本地缓存）、**零 VM 改动**；`ScanDirs`（build/test）零改 →
  **自举字节不动点 5/5 gen1==gen2 守住**。
- REPL-only：completer + 索引仅 REPL 惰性路径用。

## 待裁决

无（纯 perf、REPL-only、不动点守住、无格式 bump）。
