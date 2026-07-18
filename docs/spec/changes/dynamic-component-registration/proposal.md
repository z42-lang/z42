# Proposal: 动态组件注册（dynamic-component-registration）

> 状态：DRAFT（2026-07-17 起草）
> 子系统：`runtime`（VM 接口-cast 修复）+ `stdlib`（ComponentRegistry API + z42.build）+ `toolchain`（清单打包）
> 关系：解锁 `wire-z42b-host-build` 的 z42b↔编译器接线，取代其「z42b 静态依赖 z42c.pipeline」——**z42b 不依赖 z42c**。

## Why

z42b（构建编排器，兼反射 test-runner）要用编译器（`Z42cCompiler : ICompiler`，住 z42c.pipeline），
但**不能编译期依赖 z42c**：
1. z42b 兼 stdlib-only 反射 test-runner，静态依赖 z42c.pipeline 会破其 stdlib-only 可构建性；
2. 静态链接后，升级编译器要重编 + 重发 z42b。

**目标形态**：z42b 编译期只依赖 `z42.build`（ICompiler 接口）+ stdlib；**运行时**从 SDK `programs/`
按清单动态发现、加载、实例化编译器组件，经 ICompiler 注入。**换/升级编译器 = 替换 zpkg，宿主零改动**。

同一机制未来可注入 workload / hooks / 其他组件（provides = 不同接口）——是通用的**运行时组件注入**地基。

## 现状能力（原型已验证可复用，零改动）

z42b 已依赖 z42.test（反射 test-runner），下列 reflection MVP 原语实测全通：

| 步骤 | 原语 | 状态 |
|------|------|------|
| 加载组件 zpkg + 依赖闭包进 live VM | `Std.Test.ModuleLoader.Load(zpkg)`（`__load_module`）| ✅ |
| FQ 名 → Type（触发 lazy-load） | `Std.Type.GetType(fqn)`（`__type_get_type` → `make_type_from_name` + lazy loader）| ✅ |
| 无参实例化 | `Std.Reflection.Activator.CreateInstance(t)`（`__activator_create`）| ✅ |
| 反射静态调用（备用路径） | `__invoke_static` / `MethodInfo.Invoke` | ✅ |

## 唯一缺口（要补的 VM 件）

反射加载的实例 `o as ICompiler`（跨-zpkg 接口 cast）返回 null。

- **路径**：`interp/exec_object.rs::as_cast` → `interp/dispatch.rs::is_subclass_or_eq_td`。
- **非架构缺口**：接口元数据 zbc 1.17 已存（每类 TYPE section 存接口名，`zbc_reader` 读进
  `TypeDescCold.interfaces`）；`is_subclass_or_eq_td` 已会 fallback `try_lookup_type` 查 td。
- **待根因定位**（一次聚焦调试）：反射加载路径（test-runner 的 `load_module_from_path`）下，
  被 cast 实例的 td.interfaces() 与调用方接口名匹配不上——可能是 (a) 接口名形式 FQ vs 短名不一致、
  (b) test-loader 路径未填 interfaces、(c) `Activator.CreateInstance` 产出的实例 type_desc 未指向
  含接口的完整 td。三者任一都是**有界修复**。
- 关联 DEFERRED 的 `runtime-dynamic-load-call`（`__load_zpkg`/`__call_static` 桩），但本方案不需
  实现那两个通用桩——只需修接口 cast + 复用现有 ModuleLoader/Activator。

## What Changes

1. **VM**：修 `as_cast`/`is_instance` 对反射加载类型的接口匹配 + 单测（跨-zpkg 反射实例 `as` 接口）。
2. **stdlib `ComponentRegistry`**（住 z42.build 或新 Std.Components）：读清单 → Load → GetType →
   CreateInstance → 绑接口，封装为 `ResolveCompiler(sdkRoot) -> ICompiler`（组件缺失 → null 优雅兜底）。
3. **组件清单** `programs/components.toml`：`[component.compiler] zpkg/impl/provides`。SDK 组装时写入。
4. **z42b `_hostCompiler()`**：`ComponentRegistry.ResolveCompiler(_sdkRoot()) ?? new NoCompiler()`。
   z42b 依赖仅 +z42.build（接口）+ Std.Reflection —— **零 z42c 依赖**。

## Out of Scope
- 通用 `__load_zpkg`/`__call_static`（runtime-dynamic-load-call）——本方案不需。
- 参数化 `Activator.CreateInstance`（组件实现类须无参 ctor，如 Z42cCompiler）。
- wire-z42b 的编排 un-PARK / launcher 转发 / apphost（消费本机制，另在 wire-z42b 落）。

## GREEN 判据
- 跨-zpkg 反射实例 `as 接口` 单测绿（interp + JIT 两路）。
- 端到端：一个 stdlib-only + z42.build 的宿主，运行时注入 Z42cCompiler，编 hello → app.zpkg 且 Ok。
- self-host 7/7 不回归。

## 收益
- **解耦迭代**：升级编译器 = 替换 z42c.pipeline.zpkg，z42b 不重编不重发。
- **独立 exe**：z42b 保持 stdlib-only 可构建（test-runner 角色不破）。
- **通用地基**：workload / hooks / plugin 未来同机制注入。
