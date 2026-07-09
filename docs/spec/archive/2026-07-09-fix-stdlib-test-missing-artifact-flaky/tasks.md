# Tasks: 修 stdlib 测试"编译退出0却无产物"偶发红

> 状态：🟢 已完成 | 创建：2026-07-09 | 类型：fix（toolchain）
> 占用子系统：toolchain（ACTIVE.md 已登记）

**变更说明：** `_runUnitsBatched` 编译相位:一个单元编译进程退出码 0 但 dist 里没产出
 .zbc/.zpkg 时,当前直接进 run 相位 → runner 报 `cannot read <art>.zbc`（误导性运行期错误）。
 改为:检测"退出 0 但产物缺失" → 串行重编一次;仍缺 → 明确硬错误。

**原因：** 全 gate 高并发负载下偶发(本地 3 次 full gate 中 1 次、CI test-host 多腿),
 命中恒是最重的 crypto 测试(scrypt_vectors/secure_random_basic)。隔离(含 jobs=4)无法复现、
 编译在隔离下稳定产出且退出 0、harness 无删除产物路径、非抛异常(z42vm 抛异常退出 1 已验)、
 非 OOM(exit 137≠0)。即负载相关 Heisenbug,精确根因未定;本修复是**鲁棒性兜底**:
 把"静默偶发"转成"重试恢复 or 清晰硬错误",不掩盖真实的无产物编译。

**文档影响：** scripts/README `test` 段补一句 harness 重试语义(如需);无对外行为变更。

**局限（诚实记录）：** 本地无法稳定复现该 flaky,故**无法本地证明重试真的消除它**——
 重试是安全的严格改进(仅在已失败路径上加一次串行重编 + 更清晰的错误),不是已验证的根治。

- [x] 1.1 `xtask_test_lib_units.z42` 编译相位:okc[b] 为真但 `!File.Exists(arts[b])` → 用 `_compilePrep` 重建 Process 串行重编一次 → 仍缺则 okc[b]=false + `✗ COMPILE ERROR (no artifact after retry)` + failed++
- [x] 1.2 crypto jobs=4 回归 27/27（重试路径未触发,符合预期） 重编 xtask + `test stdlib z42.crypto --jobs 4` 回归(不回归即可,flaky 本地测不到)
- [x] 1.3 归档
