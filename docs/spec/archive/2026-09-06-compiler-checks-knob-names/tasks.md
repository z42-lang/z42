# Tasks: 编译器校验 `[profile.*.runtime]` 的旋钮名

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

## 进度概览
- [x] 阶段 1: 实现
- [x] 阶段 2: 测试
- [x] 阶段 3: 文档与验证

## 阶段 1: 实现
- [x] 1.1 新增 `z42c.driver/src/ProfileKnobs.z42`：`_validateProfileKnobs` / `_fileSettableKnobs`
      / `_isKnownKnob` / `_nearestKnob`
- [x] 1.2 `Main.z42` `_build` 在 `ManifestLoader.Load` 后立即调用校验
- [x] 1.3 `RuntimeConfigSidecar.z42` 移走 `BadKeys` 检查（改为指向新落点的注释）
- [x] 1.4 `z42c.driver.z42.toml` 加 `z42.text` 依赖

## 阶段 2: 测试
- [x] 2.1 `xtask_compiler_e2e.z42` 加 `_e2eKnobChecks`（typo / 零噪音 / 库工程旧形状）
- [x] 2.2 接进 `_testCompilerE2e`

## 阶段 3: 文档与验证
- [x] 3.1 `docs/book/src/runtime/runtime-settings.md` 诊断分工补"构建期"一层
- [x] 3.2 `src/compiler/z42c.driver/README.md` 功能索引 + 核心文件
- [x] 3.3 GREEN：test compiler / e2e / stdlib / targets / lines / dist smoke / cargo 单测
- [x] 3.4 归档

## 备注

- `build stdlib` 首轮假红（`E0401: undefined: DiagnosticCodes`）、次轮全绿——既有 backlog
  `DepScanCache` 无 mtime 守卫，本轮又咬一次（改了被其它包依赖的 stdlib/编译器包即触发）。
- `README.md` 提到的 `manifest-check` 命令在 `Main.z42` 里并不存在（roadmap 时代的遗留文案）。
  与本 change 无关，未顺手改。
