# Tasks: partial 类型协议豁免重载误报 E0433 修复

> 状态：🟢 已完成（GREEN 全绿 + gen1==gen2 3/3 + bootstrap 无越界） | 创建：2026-09-01 | 完成：2026-09-01 | 类型：fix（编译器语义） | 子系统：compiler
> 分支/worktree：`fix-partial-protocol-overload-e0433`（基于 origin/main `696d41e`）
> 本 change 是 String 补齐（阶段 2，晚一个 nightly）的前置 support；z42c 源自身不使用 → 字节中性。

## 进度概览
- [x] 根因定位：`MemberCollector._fillClass` 方法重复检测按 RegKey 判重 → 协议豁免裸键重载误报
- [x] 修复：改按完整签名判重（`_sameSignature`）+ 更新注释
- [x] collect 回归测试 ×2（协议重载放行 / 真重复仍报 E0433）
- [x] partial-types e2e ×1（协议重载共存 + 非协议 type-based 重载派发；`partial_protocol_overload` OK）
- [x] 完整 GREEN（`xtask test` 全 stage 绿，z42c [Test] 23 单元含新 collect 测试）
- [x] gen1==gen2 自举字节不动点 3/3（字节中性硬门）
- [x] `xtask test bootstrap` 无越界（nightly z42c 编当前源 OK）
- [x] 文档同步（partial-types.md E0433 语义精确化 + 协议重载可共存说明）
- [x] 归档 + PR + auto-merge

## 实现
- [x] `MemberCollector.z42`：`_fillClass` 方法分支——`ct.IsPartial && ct.Methods.ContainsKey(regName)`
      追加 `&& _sameSignature(ct.Methods.Get(regName) as MethodSymbol, msym)` 守卫；新增 private
      `_sameSignature(a, b)`（ParamCount + 各 `Z42Type.Canon(ParamTypes[i].Name())` 逐一比较）。
      字段重复检测（`:108`）不变。

## 测试
- [x] `collect_tests.z42`：`test_partial_protocol_overload_allowed`（Equals(object?)/Equals(string)
      跨碎片 → `hasCode(E0433)==false`）+ `test_partial_duplicate_method_same_sig_reports_E0433`
      （M(int) 跨碎片同签名 → 仍报 E0433）。既有 `test_partial_duplicate_member_reports_E0433`（字段）不动。
- [x] `partial_protocol_overload.z42`：partial `Tag` 两碎片；`Find(string)`/`Find(char)` 按类型派发
      （镜像 String IndexOf 重载）；`Equals(object?)`/`Equals(string)` 共存 + 可调用不崩。

## 文档
- [x] `partial-types.md`：合并语义表「重复成员」行——E0433 判据精确化为**同名 + 同完整签名**方法；
      签名不同的合法重载（含协议豁免 `Equals(object?)`/`Equals(string)`）可跨碎片共存。

## 实施纪要（2026-09-01）
- 全绿硬门：`xtask test` all stages ✔ / z42c [Test] 23 单元（含 `test_partial_protocol_overload_allowed`
  + `test_partial_duplicate_method_same_sig_reports_E0433`）/ e2e `partial_protocol_overload` OK /
  self-host 不动点 3/3 gen1==gen2 逐字节（字节中性确认——z42c 自身无协议豁免 partial 重载，`_sameSignature`
  分支在 self-build 中不触发）/ `xtask test bootstrap` 无越界。
- ⚠️ 副发现（记入 [[augment-string-prelude]]，约束 Change 2）：partial v1 的重载 mangle 预扫描是**每-fragment**，
  同名 type-based 重载对分处不同碎片会派发错误 → String 补齐拆分时每组重载（IndexOf/Split/Equals 对）必须同碎片。

## 备注
- 自举能力版本号不 bump（无新语法/格式）。
- String 补齐（阶段 2）在本 change 进 nightly 后于独立 change 落地（worktree `../z42-straug`，
  草稿存 scratchpad `string-drafts/`）。
