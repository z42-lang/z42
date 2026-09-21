# use-free-function-overloads — 实施任务

## 前置确认

- [x] **P1 nightly 条件**：确认阶段 1（#731）已随 nightly 发布（nightly `main @ b82282d` 历史含 #731），
      「晚一个 nightly」满足 ⇒ 阶段 2 可 use。
- [x] **P2 全库扫描**：stdlib / z42c / 工具链三路穷尽扫描候选，筛选出 6 组（全在工具链），
      排除正当分名（详见 proposal.md）。

## 合并（6 组，7 文件）

- [x] **T1 组 1** `_orchestrate`×3（builder.z42）：`_orchestrateFor`/`_orchestrateWith` → `_orchestrate`；
      调用点 builder.z42 内 2 处 + builder_test.z42:216。
- [x] **T2 组 2** `_initialInputs`×2（builder.z42）：`_initialInputsWith` → `_initialInputs`；调用点内部 2 处。
- [x] **T3 组 3** `_forwardZ42b`×2（launcher_cli.z42）：`_forwardZ42bEnv` → `_forwardZ42b`；调用点 launcher_export.z42:188。
- [x] **T4 组 4** `_resolveDevTargets`×2（builder_dev_targets.z42）：`_resolveDevTargetsF` → `_resolveDevTargets`；
      调用点 builder_test.z42:89。
- [x] **T5 组 5** `_dtSort`×2（builder_dev_targets.z42，类型重载）：`_dtSortByName`/`_dtSortStrings` → `_dtSort`；调用点内部 2 处。
- [x] **T6 组 6** `_pubPlatform`×2（builder_publish.z42，类型重载）：`_pubPlatformStr`/`_pubPlatformBool` → `_pubPlatform`；
      调用点 builder_publish.z42 内 6 处 + builder_device_ios.z42:82。
- [x] **T7 注释同步**：各定义处更新注释说明「阶段 2 use，此前因不支持自由函数重载而分名 X」（保留旧名于说明文字）。

## 验收（GREEN）

- [x] **G1 build toolchain 编译通过**：含 #731 support 的自建 z42c 全量重编工具链源（`cached: 0/N`），
      零编译错误、零 E0408/E0425/ambiguous。类型重载（组 5/6）编译通过即证 OverloadResolver 选对
      （末参 `false`→bool 重载、`files`→string[] 重载，否则形参类型不符会报错）。
- [x] **G2 工具链行为不变**：`test targets` 全绿——`_buildZ42bWith` 用当前 z42c **现编** z42b（我的重载源），
      `list targets`（组 4/5 `_resolveDevTargets`/`_dtSort`）+ `harness=false`（组 1/2 `_orchestrate`/`_initialInputs`）
      + bundle-host 全 pass。
- [ ] **G3 GREEN**：`xtask test changed`（按改动区选工具链相关测试）。
- [ ] **G4 冷启动**：`compile-toolchain`（CI 验；本地 test bootstrap 不编工具链源，line 287）。

## 文档

- [x] **B1** change 目录 proposal.md + tasks.md（全库扫描结论 + 6 组清单 + 排除项 + 验收）。
- [ ] **B2** doc-check 三问：外部可见？否（工具链内部重构，特性阶段 1 已文档化）→ 无 learn/reference 改动；
      接手可懂？代码注释 + 本 change 记录已覆盖 → 无额外 internals 页；目录/入口未变 → 无 README 改动。
