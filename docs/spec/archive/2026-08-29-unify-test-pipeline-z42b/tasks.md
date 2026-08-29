# Tasks: 统一测试流水线归 z42b（第一步）

> 状态：🟢 已完成 | 创建：2026-08-29 | 完成：2026-08-29

## 进度概览
- [x] 阶段 1：源码归位（testhost → workload/test）
- [x] 阶段 2：构建接线 + 文档
- [ ] 阶段 3：验证与归档

## 阶段 1：源码归位
- [x] 1.1 `git mv` `src/toolchain/testhost/agent/src/agent.z42` → `src/toolchain/workload/test/agent/src/agent.z42`
- [x] 1.2 `git mv` toml → `workload/test/agent/`（include `src/**/*.z42` 相对 toml 目录，整目录同搬 → 无需改）
- [x] 1.3 删除空的 `src/toolchain/testhost/`（含 README.md）

## 阶段 2：构建接线 + 文档
- [x] 2.1 `scripts/test/xtask_test_embedded.z42` 第 19–20 行 toml/dist 路径 → `workload/test/agent`
- [x] 2.2 新建 `src/toolchain/workload/test/README.md`（六段制）
- [x] 2.3 `src/toolchain/workload/README.md`：登记 test 能力 workload + 澄清含平台无关共享束
- [x] 2.4 `docs/design/testing/embedded-app-run.md`：impl 路径 + 统一流水线框架
- [x] 2.5 `docs/design/testing/cross-platform-testing.md`：两层模型 + bundle 缝 + 分阶段 + test workload 形状
- [x] 2.6 `docs/roadmap.md`：登记本 change + 后续阶段索引（含阶段 2「简化 xtask→z42b」一等 follow-up）
- [x] 2.7 （Scope 扩展）`.github/workflows/ci.yml`：paths-filter 删 stale `src/toolchain/testhost/**`

## 阶段 3：验证与归档
- [x] 3.1 `xtask test embedded`：agent 从新路径 `workload/test/agent` 构建成功（越过 `_ensureTestAgent` 进语料构建）；余下 embedded 因 nightly 种子 stale 视角出现无关假阳性（见备注），非本 change
- [x] 3.2 `xtask test`（完整 GREEN gate）—— ✅ 全绿 all stages passed (C#-free)
- [x] 3.3 spec 目标一致性确认（两层模型/bundle 缝/分阶段/workload 归位落文档）
- [x] 3.4 doc-check：`grep testhost` 仅剩本 change 归档 + 有意历史说明；死路径已清（含 embedded-app-run.md paths-filter 描述）
- [x] 3.5 归档 + PR

## 备注
- 本步零 CLI / 零打包改动；「怎么下载」（payload-only workload 打包 + manifest）落后续阶段（design D6）。
- 产物 zpkg 名 `z42.testagent.zpkg` 与 agent namespace 本步不动。
- **本地种子坑（记录）**：主树 seed(Aug-24)早于 origin/main 两处零格式-bump 变更——`shrink-primitive-native-interop` 删 `__int32_equals` builtin(Aug-28)+ z42.ir 库命名空间收敛(`8b4aae94`)。用旧种子直建撞 `unknown builtin` / E0436；**换 nightly SDK 种子 + 清 `artifacts/build` stale 产物后 `xtask test` 全绿**。`xtask test embedded` 残留的 `z42.ir/zpkg` E0436 经查为 nightly 种子旧命名空间视角的**假阳性**（该文件用到的类型在当前 origin/main 全在 `Z42.IR`/`Z42.Project` 且已 import），非真 bug，不修。
