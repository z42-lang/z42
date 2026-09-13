# Tasks: guard-depscan-cache-staleness

> 状态：🟢 已完成 | 创建：2026-09-13 | 完成：2026-09-13
> 类型：`fix`（compiler）—— 走[最小化模式](../../../../.claude/rules/workflow.md)（无 proposal/spec/design）。

**变更说明：** `DepScanCache.Get` 从「按绝对 path 缓存」改为「path + (size, mtime_ms) 守卫」，
命中但文件被覆写过即重读重解、并作废该条目派生的 Tsig/Mods/Types。

**原因：** 这是 `.claude/rules/bootstrap-seed.md` 里挂着的 backlog 根因。`DepScanCache` 自己的注释
就写着「若将来新增『进程内覆写 zpkg 后重扫』路径，需在 Get 加 mtime/size 守卫」—— 而 CI 的两代
自举**就地覆写** `artifacts/build/{libraries,compiler}`，早就是那条路径：gen1 起步时 artifacts 还是
gen0 的旧 minor 产物 → `ZpkgReader.Open` strict-pin 返 null → **缓存 null** → 覆写成新 minor 后
后续成员复用那个 null → 跨包类型 undefined（E0401/E0443）。症状是「2026-08-21 (#240) 之后任何格式
bump CI 全红」，一直靠 `ci-bootstrap` §1.5「每代构建前清空 artifacts」兜着。

**文档影响：** `.claude/rules/bootstrap-seed.md`（backlog → 已修 + 残余窗口 + CI 清理的退休触发条件）。

- [x] 1.1 `CachedZpkg` 加 `Size` / `MtimeMs`；`DepScanCache.Get` 命中时比对，不同即就地换新条目
      （而非追加一条，避免同 path 两份缓存）
- [x] 1.2 mtime 走 `[Native("__file_last_write_time_ms")]` 直绑 —— 与同包 `NsIndexCache._mtimeMs`
      同款，不引 `z42.time` 依赖（轴 ③：z42c 源不得新用未随 nightly 发布的 stdlib API）
- [x] 1.3 stat 失败（文件刚被删 / 无权限）容错：size = -1 ⇒ 命中照旧返回缓存（= 守卫前行为，
      不把一次良性命中变成硬失败）；存下的 -1 与任何真实 size 都不等，下次 stat 成功即自愈
- [x] 1.4 回归测试 `src/compiler/z42c.pipeline/tests/depscancache/`，按**真实形状**复现
      （旧 minor 头 → Open 返 null 被缓存 → 就地覆写成当前 minor → 必须重解拿到非 null）
- [x] 1.5 **阴性对照**：把守卫关成 `if (true) return hit;` → `test_overwritten_zpkg_is_reopened…`
      判红、`test_untouched_zpkg_still_hits_the_cache` 仍绿（后者守的是「别矫枉过正成每次重解」，
      那会把 F2 省下的 O(N²) 解码原样还回去）
- [x] 1.6 文档同步：`.claude/rules/bootstrap-seed.md`

## 备注

**没做、且是有意的**：没有顺手删掉 `ci-bootstrap` §1.5 的清理。那条路径**本地不可验**
（要冷启动 + 格式 bump 才走到），现在删等于拿一次真实 bump 当验证。退休的触发条件已写进
bootstrap-seed.md：**下一次格式 bump 的 PR 里顺手删掉并观察 CI** —— 那时它正好被真实行使一次。

**残余窗口**：mtime 只有毫秒粒度，「同毫秒内覆写成同样大小的另一份内容」测不出来。这是
make / ninja / rustc 同款取舍（消掉它只能算内容哈希，等于把缓存省下的 I/O 又付回去）。
两代自举之间隔着整轮构建，不在这个窗口里。

**同族未查**：本次只动 `DepScanCache`。`CacheStore` / `NsIndexCache` 等其它进程级缓存是否有同款
缺口没有逐个核 —— 属「二轮评审架构剩余 A1：依赖扫描四层缓存合一」那条，独立处理。
