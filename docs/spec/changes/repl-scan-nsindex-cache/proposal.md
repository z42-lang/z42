# Proposal: REPL 首次求值提速——惰性 prelude（T1）+ ns 索引持久化（B）

> 状态：🟢 IMPL 完成 | 创建：2026-07-31 | 交付：T1（lazy-prelude）+ B（ns-index）
> 类型：perf（compiler 子系统 `DepScan.ScanDirsLazy` + toolchain `Script.Prewarm`；仅 REPL 路径，
> **不碰非惰性 `ScanDirs`（构建路径）→ 自举 byte-identical 不受影响**）
> 子系统：`compiler`（z42c.pipeline `DepScan`）+ `toolchain`（z42.scripting `Script`）
> + 借道 `z42.ir`（`LazyReconWorld` 惰性开包，同 lazy-type-world 手法）
>
> **核心洞察（对照 Python）**：`z42 repl -c "1+1"` = 1.7s，而 Python `1+1` = 0.01s、z42 VM 裸启动 = 0.00s。
> 差距全在「每次求值都跑整个 AOT 编译器 + **eager reconcile 整个 prelude+usings 类型世界**」——`1+1` 只需
> 内建 `int`+算子，却拖动整个 stdlib。Python 快在**解释 + 运行期惰性名字解析**，对 `1+1` 根本不干这些活。
> 本 change 学 Python：**对 `1+1` 别干这些活**（T1 惰性 prelude），而非把活缓存起来提前干（原 C 方案否决）。

## 背景与动机

REPL 首次求值在 macOS 暖态实测 **1.75s**（`z42 repl -c "1+2"`），Windows 冷启动 + Defender 更糟。
带时间戳插桩把 `ScanDirsLazy` + Prewarm 的成本切开（macOS 暖态）：

| 阶段 | 耗时 | 说明 |
|------|------|------|
| 启动（VM+z42i 加载） | 0.10s | 不在本 change 范围 |
| **scan: open-all** | **0.375s** | `ReadAllBytes+Open` 全部 26 个 lib 包（仅为建 ns 路由） |
| **scan: nsmap+prelude 重建** | **0.59s** | `ReadNamespaces`×26 + prelude(z42.core) 的 `ReadModuleSigs+Rebuild` |
| **默认 usings 预载** | **0.30s** | 4 个默认 using 包各 `EnsurePackageLoaded`（Rebuild） |
| 编译 `1+2` | 0.37s | 不在本 change 范围 |

交互模式下预热 worker 与打字并行，间隙（>1.4s）能**完全隐藏** scan+usings → 首句 0.37s（实测）。
但**一次性 `-c` / 秒贴 / 冷启动 / 快速打字（<1.3s）** 时这 1.27s 暴露为可感延迟。且首次 eval 会
**干等 Prewarm 发布 CachedScan**，而 Prewarm 把 scan(0.96s) 和 usings(0.30s) 串成一段才发布——
哪怕 `1+2` 不引用任何 using，也白等那 0.30s。

## 目标：T1（惰性 prelude）为主 + B（ns 索引）为辅

**不改变任何 eval 结果**、**不碰非惰性 ScanDirs（构建路径）**、**不碰并发模型**（无后台 reconcile）：

- **T1（惰性 prelude）——核心**：`ScanDirsLazy` **不再 eager reconcile prelude(z42.core)、Prewarm 不再
  预载默认 usings**。worker 只建**骨架**（nsMap 路由 + 惰性 world）→ 秒发布。`1+1` 类纯表达式**零包加载**
  → **1.72s→0.40s**（秒回车也不卡）。首次引用 Std 符号时 `_compileSrc` 既有的 E0401 回退按需加载 prelude
  +usings（默认 using `Std.Collections` 由 z42.core 声明 → 加载它即把 prelude 一并 reconcile；实测
  Convert/Environment/Object 正确）。**race-free**：worker 建完即发布退出，无「发布后变异 scan」。
- **B（ns 索引持久化 → 惰性开包）——辅**：`ScanDirsLazy` 把「每 zpkg → 命名空间列表」落盘缓存（按 libs
  指纹 key）；命中则从缓存建 nsMap + `LazyReconWorld` 路由，**不再 open-all 全部 26 个包**，只按需 `Open`
  引用闭包。使 T1 的骨架 scan 从 ~0.4s（open-all）降到 ~0.05s（命中），且**是 Windows 的对症解**（消除
  20+ 次被 Defender 逐个扫的文件打开）。

### 为什么不后台 reconcile（曾试双缓冲，否决）

理想是「骨架秒发布 + 后台用打字间隙 reconcile 完整世界」使首次符号 eval 也快。**实测走不通**：本 VM 的
GC 安全点协作会让**计算密集的后台线程阻塞主线程 eval**——主线程 `1+1` 编译分配 → 触发 GC → 死等后台
reconcile 线程到安全点（~3s）。故 `1+1` 秒回车反被后台预热拖到 3s。（旧 prewarm 能藏住 reconcile 仅因
readline 是 native-park 释放 GC；计算不 park。）→ 放弃后台 reconcile，首次符号 eval 走前台 E0401 兜底。

## 收益（macOS 暖态实测）

| 场景 | 改前 | T1+B 后 |
|------|------|---------|
| `1+1` 秒回车（即时） | 1.7s | **0.40s**（4.3×） |
| `-c "1+2"` | 1.75s | **0.40s** |
| 首次 `Console.WriteLine` | 1.7s（打字间隙时 0.36s） | 1.71s（前台按需，**权衡**——见下） |
| Windows 冷启动 | 严重（逐文件 Defender 扫） | 大幅改善（ns 索引免 open-all） |

**权衡（如实）**：首次**符号**求值（Console/List）不再被后台预热隐藏，走前台按需 reconcile ≈ 1.7s。对
「先 `1+1`」的用户全赢；对「先 `Console.WriteLine` 且会停顿」的用户从 0.36s 回归到 1.7s。根因是
reconcile 仍是**整包/整闭包**粒度——彻底解由 T2 承接。

## 非目标（留作后续独立 change T2）

- **T2 按符号 reconcile**：用 `Console` 只 materialize `Console` 一个类型（+基类链），不 reconcile 整个
  Std.IO+prelude 闭包 → 首次符号 eval 降到 ~0.5s，**且不需后台线程**（避开 GC 坑）。调研结论：这是
  **编译器名字解析核心**改动（`SymbolTable` 惰性 miss 回调 + 按类型 zpkg 读 + 解决 arity-mangle/impl 合并/
  接口顺序等不按类型分解的整包问题），miscompile 风险高，需独立 spec + 完整门禁。见 tasks.md「T2 调研」。

## 正确性门禁（GREEN）

- **自举 byte-identical 不动**：非惰性 `ScanDirs`（`z42c build` 走它）零改动 → gen1==gen2 天然不受影响。
- **REPL eval 正确性**：一组代表性 eval（纯表达式 / `Console.WriteLine` / 集合 / 顶层声明 / 跨轮 var carry）
  结果与改前逐一致。
- **缓存命中 vs 未命中一致**：同一 libs 下，命中缓存路径与回退 open-all 路径产出的 nsMap / Exported 一致。
- **缓存失效**：libs 指纹变更（改一个 zpkg）→ 索引自动重建（不用陈旧路由）。
- `xtask test`（e2e + cross-zpkg + stdlib + runtime）全绿。
