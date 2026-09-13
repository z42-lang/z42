# Tasks: 缓存 zpkg 候选扫描 + 去掉重复的索引构建（cache-zpkg-candidate-scan）

> 状态：🟢 实施+验证完成，待合并 | 创建：2026-09-04 | 类型：perf / refactor（不改可观察语义 → 最小化模式）
> 依赖：基于 `perf-boot-static-init`（PR #418）分支，收益在其之上度量。

**背景（实测，hello world 启动 15.5 ms）**：

```
[phase] declared_candidates(11) = 5.8–10.1 ms   ← 最大头
[idx] 911 funcs 3151 blocks total=0.75ms        ← z42.core 加载时
[idx] 912 funcs 3152 blocks total=0.65ms        ← 合并后**又跑一遍同一批函数**
[phase] resolve_module = 0.22ms
```

**根因 1**：`namespace_index::scan_zpkg_candidates` 会把 libs 目录下**全部 25 个 zpkg**
逐个 `read` + 解析 NSPC；而 `loader::namespace::resolve_namespace` 每次调用都做一次完整扫描，
`app::build_declared_candidates` 又**按入口的每个 import 命名空间各调一次**
（hello 是 `Std` / `Std.IO` / `Std.IO.Console` 三个）→ **75 次读+解析**。
单个候选的读+解析只要 0.01–0.11 ms（11 个合计 0.6 ms），成本全在重复扫描。

**根因 2**：`build_block_indices` 对同一批函数跑两遍——z42.core 作为 artifact 加载时一遍，
合并进入口模块后又一遍（911 vs 912 个函数，第二遍 0.65 ms 纯浪费）。

## 实施

- [x] 1.1 `namespace_index`：按**文件路径 + (size, mtime)** 记忆化单个 `ZpkgCandidate`
      （守卫是必须的——`DepScanCache` 的 path-only 缓存曾因此读到旧导出面，见
      `GeneratorLoader.z42` 的注释）。重复扫描退化为 readdir + 每文件一次 stat。
- [x] 1.2 `scan_zbc_candidates` **不做**：走 module_paths（运行时通常为空），且 zbc 只声明单个命名空间，
      重复扫描不是热点。留待有数据再说。
- [x] 2.1 `build_block_indices`：跳过**已建过索引**的函数（合并后的第二遍）
- [x] 2.2 判据 = `!blocks.is_empty() && branch_targets.len() == blocks.len()`——
      本函数一定把 `branch_targets` 填成与 `blocks` 等长，手工构造的测试函数两者都为空 → 不跳过

## 验证

- [x] 3.1 `cargo test --lib` 996 + 21 passed；wasm32 检查 0 error
- [x] 3.2 `xtask test` ✅ GREEN（283 + 16 + 2 passed，0 failed）+ REPL 多轮冒烟正常
- [x] 3.3 A/B：hello / regex / z42i / z42c 编译（hyperfine ≥ 40 runs + peak RSS）
- [x] 3.4 z42c 自编译产物字节相同

## 实测（同机 hyperfine 40–50 runs，基准是 #418 的 VM）

| 场景 | main | #418 | 本变更 | 相对 #418 |
|---|---|---|---|---|
| hello 墙钟 | 29.8 ms | 14.6 ms | **10.7 ms** | 1.37× |
| 用 Regex 的程序 | — | 19.2 ms | **12.0 ms** | 1.60× |
| `z42i -c '1+2'` | 444.8 ms | 68.1 ms | 66.1 ms | 持平 |
| z42c 编译 hello | — | 461.3 ms | 461.6 ms | 持平 |

hello 累计 **29.8 → 10.7 ms（2.80×）**。z42i / z42c 持平是预期内的——它们的耗时主要在编译管线自身。

## 覆盖缺口（明说）

**mtime 守卫没有单元测试**：要构造一个「合法 zpkg 字节」才能走到缓存分支，而 zpkg 的写入端在
z42 侧（`ZbcWriter`），Rust 单测里手搓字节既脆又长；依赖 `artifacts/` 里的真实 zpkg 又会让
`cargo test --lib` 依赖构建产物。目前靠 `xtask test` 的 z42b「先编译后加载」流程间接覆盖。
若要补，正确做法是给 `namespace_index` 加一个能注入 stamp 的测试钩子，而不是造字节。

## 被否决的做法

- **per-function 派生数据惰性化**（原本的候选方案）：实测只值 0.75 ms（z42.core 的 911 个函数
  合计 `block_index` 0.14 / `branch_targets` 0.06 / `fused_tails` 0.26 / `frame_meta` 0.11 / `max_reg` 0.06 ms），
  却要改 `Function` 字段布局 + 碰 interp 热路径。性价比远低于本变更的两条，暂不做。
- **`ZpkgCandidate::build` 分段读**：11 个候选的读+解析合计只有 0.6 ms，且它带 zpkg 版本校验；
  真正的成本在**重复全目录扫描**，已由 1.1 解决。
