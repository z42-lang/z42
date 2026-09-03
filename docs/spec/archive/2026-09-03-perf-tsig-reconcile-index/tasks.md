# Tasks: TSIG 重建去平方（perf-tsig-reconcile-index）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：perf（z42.ir + z42c.pipeline；三面评审 C-8 的根因版）
**变更说明：** `TsigReconcile` 每个类 ① 在整个 world（全部包 × 模块 × 类）按名线性 `_locate` / 基链定位，② 每个祖先层
扫祖先模块**全部** SIGS 函数做 `StartsWith`。两者随 world 规模平方增长。新增 `TsigIndex.z42`：`ReconClassIndex`
（类 FQ → (包,模块,类)，模块进入 `LazyReconWorld.Wp` 时登记，重名保留 (p,m,t) 最小者 = 原 first-wins）与
`SigsClassIndex`（每 `ZpkgModuleSigs` 按类分桶的函数链，懒建、桶内保持原下标序）。另加 `Z42C_TRACE_DEPSCAN=1`
把 DepScan 的 open / sigs / tsig 三段耗时打到 stderr（本次测量口径）。
**原因：** 报告 C-8 把它记为「每包固定 ~0.8 s、建议落盘缓存」；实测三段占比 open 73 ms / sigs 140 ms / **tsig 939 ms**
（25 包 world，hello-world 编译总 1.6 s），根因是平方级扫描而非「重建本身必要的开销」——先修算法，再决定是否还需要缓存。
**文档影响：** `src/libraries/z42.ir/README.md`（核心文件 + 功能索引）、`docs/book` 的 TSIG/依赖扫描机制页、
`src/compiler/z42c.pipeline/README.md`（trace 环境变量）。

- [x] 1.1 `TsigIndex.z42`：`ReconClassIndex` / `SigsClassIndex`；`ZpkgModuleSigs` 增 `ByClassHead/ByClassNext`
- [x] 1.2 `LazyReconWorld`：`_cls` 索引，`EnsureNs` 追加模块时登记、`FromEager` 全量登记；`FindClass/FoundPkg/FoundMod/FoundDesc`
- [x] 1.3 `_rebuildClass` 基链定位与两趟 SIGS 扫描改走索引；`_locate` 改 O(1)
- [x] 1.4 `DepScan.ScanDirs` 三段计时（`Z42C_TRACE_DEPSCAN=1`）
- [x] 1.5 整包 TYPE/SIGS 只解析一次：`CachedZpkg.Types` memo + `LazyReconWorld.Preset` / `ReconPreset`（EnsureNs 按 ns 合并，规则同 ReadOne*）+ 5 参 `Rebuild`
- [x] 2. 对比数据：`Z42C_TRACE_DEPSCAN` 三段耗时（hello-world / z42c.semantics）；`build stdlib` 整体墙钟；hyperfine 单包编译
- [x] 3. 字节对账：用 base 编译器（main 9b4ac4a5 工具链）与本分支编译器分别编全部 stdlib 包，逐包 `cmp`
- [x] 4. `xtask test` GREEN（含自举不动点）
- [x] 5. 文档同步 + 归档

## 备注
- 等价性论证：① `_locate` 原按 p→m→t 升序 first-wins，索引在 `Add` 冲突时保留字典序最小三元组；② SIGS 桶键 = 函数名最后一个 '.' 之前，
  与原 `StartsWith(cls+".") && mkey 无 '.'` 完全等价；桶内逆序头插 → 正序遍历，保持原下标序（override 覆盖位次不变）。

## 对比数据（2026-09-03，macOS arm64，机器另有一个 GREEN 在跑，绝对值偏高、比例可信）

`Z42C_TRACE_DEPSCAN=1` 三段耗时（hello-world `--emit-zbc`，libs = 25 zpkg，3 次取典型值）：

| 版本 | open | sigs（DepIndex）| tsig（Rebuild）| DepScan 合计 | 编译总墙钟 |
|---|---|---|---|---|---|
| base（main 9b4ac4a5）| 73 ms | 140 ms | 939 ms | 1152 ms | 1.70 s |
| + 两张索引（1.1–1.3）| 75 ms | 144 ms | 400 ms | 619 ms | 1.18 s |
| + 整包只解析一次（1.5）| 75 ms | 86 ms | **133 ms** | **294 ms**（**3.9×**）| **0.94 s**（1.8×）|

产物：hello-world zbc 与 base 逐字节相同。全 stdlib 逐包字节对账与 `build` 墙钟见下（bytecmp）。

全 stdlib 逐包对账（`bytecmp.sh`：同一源码树 wt-c8、同一 z42vm，base = main 9b4ac4a5 自建工具链 vs 本分支工具链，
每包 `z42c build <toml> --release` 到独立目录后 `cmp` zpkg）：**25/25 逐字节相同**（含 z42.ir 自身）。
逐包墙钟合计（含 VM 启动 + DepScan + 编译 + 写包；机器同时跑另一 GREEN）：base 73.6 s → new 54.7 s（**−26%**）；
小包（uri/random/text 等）2.2–2.4 s → 1.4–1.6 s（每包固定省 ~0.8 s），大包（core/ir）5.1 s → 4.4–4.5 s。
