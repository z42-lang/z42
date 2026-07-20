# Tasks: 接口成员枚举（typeof(IFoo).GetMethods()）

> 状态：🟡 实现完成，验证受阻于环境（stale 0.32 seed vs 0.33 源）| 创建：2026-07-17 | 实现：2026-07-20
> **重定为纯 runtime**（勘察见 proposal）：`fix-crosspkg-interface-impl` 已 emit 接口方法块（zbc 1.28），reader 原丢弃 → 改存储 + GetMethods 表面化。**无 compiler 改动、无格式 bump。**
> 子系统锁：**runtime**（未争用）。

## 进度概览
- [x] 阶段 0: 勘察——格式与发射已就位（zbc 1.28），本变更纯 runtime
- [x] 阶段 1: reader 存储接口方法块
- [x] 阶段 2: GetMethods 表面化
- [x] 阶段 3: 测试 + 文档
- [ ] 阶段 4: GREEN 验证（**本地受阻，见备注**）+ 归档

## 阶段 1: reader 存储（was discard）
- [x] 1.1 `bytecode.rs`：`ClassDesc.iface_methods` + `IfaceMethodSig` 结构（name/ret/param_types）
- [x] 1.2 `zbc_reader.rs`：接口方法块 parse+discard → 读入 `iface_methods`
- [x] 1.3 `types.rs`：`TypeDescCold.iface_methods` + accessor
- [x] 1.4 `loader.rs`：`desc.iface_methods` → cold + emptiness 检查
- [x] 1.5 `bytecode.rs` TypeDescCold rebuild clone 补字段

## 阶段 2: GetMethods 表面化
- [x] 2.1 `reflection.rs`：`build_iface_method_info`（从 sig 直建，IsAbstract/IsVirtual=true，无 backing Function）
- [x] 2.2 `builtin_type_methods`：接口分支（td.iface_methods() → MethodInfo）

## 阶段 3: 测试 + 文档
- [x] 3.1 reflection.z42：`IShape`/`IExtendedShape` + `test_interface_getmethods_declared` / `_declared_only`
- [x] 3.2 test 结构体字面量补 `iface_methods`（loader/merge/constraint _tests）
- [x] 3.3 reflection.md：接口成员枚举落地
- [x] 3.4 roadmap.md：0.3.12 退出标准「接口成员枚举」标 ✅（line 107 + 386）

## 阶段 4: 验证 + 归档
- [x] 4.1 `cargo build --release`（z42vm）无错
- [x] 4.2 `cargo check --tests` 无错（补齐字段后）
- [ ] 4.3 完整 `./xtask test` —— **本地受阻**（非本变更）
- [ ] 4.4 归档 + roadmap

## 备注：本地 GREEN 受阻（环境，非本变更）
并发 `fix-crosspkg-interface-impl`（2026-07-18）把格式 bump 到 **zpkg 0.33 / zbc 1.28**，但本机
`.z42` SDK 种子（Jul 15）+ in-tree artifacts 都是 **0.32**——`z42c self-build` 用 0.32 种子，
新 vm（0.33 reader）strict-pin 读不了 → build stdlib 失败。**这对任何变更都一样**（本变更纯 runtime、
零 compiler 改动、格式不动，自举天然不受影响）。CI 的两代自举（fix-bootstrap-format-bump-deadlock）
会自动吸收该 bump 并验证。本地要验需：刷新 nightly 种子到 0.33（网络）+ 清 in-tree 重建，或两代自举。
