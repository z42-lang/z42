# Design: 动态组件注册

> 运行时从 `programs/` 按清单加载、实例化实现某接口的组件，经接口注入宿主。
> 首个消费者：z42b 注入 `Z42cCompiler : ICompiler`（z42b 不依赖 z42c）。

## 架构

```
z42b (stdlib-only + z42.build 接口)
  └─ _hostCompiler()
       └─ ComponentRegistry.ResolveCompiler(sdkRoot)          [stdlib]
            1. 读 programs/components.toml → {zpkg, impl, provides}
            2. ModuleLoader.Load(sdkRoot + "/programs/" + zpkg)   [__load_module —— 现有]
                 → 组件 zpkg + 依赖闭包（z42c.semantics/ir/project/…）进 live VM
            3. Type.GetType(impl="Z42.Pipeline.Z42cCompiler")     [lazy-load —— 现有]
            4. Activator.CreateInstance(t) → object               [无参 ctor —— 现有]
            5. (ICompiler) o                                       [跨-zpkg 接口 cast —— 待修 VM]
       └─ 得 ICompiler → 注入 Pipeline.Compiler → 8 相位编排
```

依赖方向（无环，且 z42b 无 z42c 边）：
```
z42b        → z42.build（接口） + z42.test（ModuleLoader） + Std.Reflection   [编译期]
z42c.pipeline（Z42cCompiler）→ z42.build（接口）                              [已存在，2a]
z42b ⇢ z42c.pipeline                                                          [仅运行时动态，无编译期边]
```

## Decisions

### D1: 组件清单驱动，不硬编码路径

**问题**：z42b 怎么知道去哪加载、加载什么类？

**决定**：`programs/components.toml`（SDK 组装时写）声明每个组件：
```toml
[component.compiler]
zpkg     = "z42c/z42c.pipeline.zpkg"   # 相对 programs/
impl     = "Z42.Pipeline.Z42cCompiler" # 实现类（须无参 ctor）
provides = "Z42.Build.ICompiler"       # 契约接口
```
换编译器实现 / 升级 = 改这一段 + 放新 zpkg，**宿主与注册表代码零改动**。`provides` 供注册表校验
（可选）+ 未来按接口查询（`Resolve(provides="...")`）。

### D2: 复用 ModuleLoader.Load，不做新 __load_zpkg

**问题**：加载组件 zpkg 用现有 `ModuleLoader.Load` 还是补 DEFERRED 的 `__load_zpkg`？

**决定**：复用 `ModuleLoader.Load`（`__load_module`，z42.test，z42b 已依赖）。原型实证：它把组件 zpkg +
**依赖闭包**注册进 lazy loader（含 dep 候选 + namespace 映射），之后 `Type.GetType` 能解析组件类。
通用 `__load_zpkg`/`__call_static`（runtime-dynamic-load-call）**不需实现**——省一大块 runtime 工作。

> ⚠️ 命名：`ModuleLoader.Load` 现属 z42.test（test-runner 语义）。本机制复用它属**合理泛化**；
> 若嫌语义耦合，可另在 z42.build/Std.Components 暴露一个瘦封装 `Assembly.Load`（同 native `__load_module`），
> z42b 依赖它而非 z42.test——待实现期定（不影响机制）。

### D3: 唯一 VM 修复 —— 反射加载类型的接口 cast

**问题**：`o as ICompiler`（o 由反射从组件 zpkg 加载+实例化）返回 null。

**根因区间**：`as_cast`（`interp/exec_object.rs`）→ `is_subclass_or_eq_td`（`interp/dispatch.rs`）：
```rust
let td = registry.get(cur).cloned().or_else(|| ctx.try_lookup_type(cur));  // 已 fallback lazy loader
if td.interfaces().iter().any(|i| iface_reaches_td(ctx, registry, i, target)) { return true; }
```
机制已在（zbc 1.17 存接口、reader 读进 `TypeDescCold.interfaces`、check 已 fallback）。cast 仍失败 →
**一次聚焦调试**定位三候选之一：
1. **接口名形式**：td.interfaces() 存的是 FQ（`Z42.Build.ICompiler`）还是短名（`ICompiler`）？
   AsCast 的 `target`（`class_name`）是哪种？两端须一致（修哪端由实证定；倾向都规整为 FQ）。
2. **reflective-load 路径填 interfaces 否**：`load_module_from_path`（test-loader）与 `load_zpkg_file`
   （全量）是否都把 TYPE section 的 interfaces 灌进 TypeDesc？若 test 路径省了 → 补。
3. **实例 td 指向**：`Activator.CreateInstance` 产出的 `Value::Object` 的 `type_desc().name` 是否解析到
   **含接口的完整 td**（而非 name-only 合成）？

**验证**：加单测——一个测试 zpkg 定义 `class Impl : IFace`，主程序 `ModuleLoader.Load` 它 →
`Activator.CreateInstance` → `o is IFace` 应为 true（interp + JIT 两路，jit/helpers/object.rs 同步）。

### D4: 优雅兜底 + jit 一致

- 组件缺失（runtime-only SDK 无 programs/z42c）→ `ResolveCompiler` 返回 null → z42b `?? new NoCompiler()`，
  build 动词报「此 SDK 无编译器组件」而非崩。
- VM cast 修复须 interp + JIT 双路一致（`jit/helpers/object.rs::jit_as_cast`/`jit_is_instance`）。

## 数据流：z42 build hello

```
z42 build hello           [launcher AddSpawn → z42vm z42b.zpkg -- build hello]   (wire-z42b 2c)
  └─ Z42Builder build
       ManifestLoader.Load(hello/…toml)                    [z42.project]
       ctx = {Dirs, Target(host,debug), Inputs.Deps}
       p = new Pipeline(); p.Compiler = _hostCompiler()    [← ComponentRegistry 动态注入 Z42cCompiler]
       p.Run(ctx):
         Compile: ctx.Compiler.Compile(req)                [Z42cCompiler → PackageCompile → app.zpkg]
         Trim/Assets/…                                     [骨架；plain build 近 no-op]
  → dist/hello.zpkg
```

## 落地顺序
1. **VM cast 修复**（D3）+ 跨-zpkg 反射 `is`/`as` 单测（interp+JIT）。← 本 change 核心
2. **ComponentRegistry**（stdlib）+ `components.toml` 格式 + SDK 组装写清单（toolchain）。
3. **z42b 接线**（wire-z42b 2b）：`_hostCompiler` 接 ComponentRegistry；z42b 依赖 +z42.build、**无 z42c**。
4. **端到端**：`z42b build hello` 动态注入编译 → app.zpkg（wire-z42b 2c/2d 续 launcher/apphost）。

## Testing
- VM：跨-zpkg 反射实例 `is`/`as` 接口单测（Rust + z42 层）。
- stdlib：ComponentRegistry 读清单 + 解析（缺失兜底、坏清单报错）单测。
- e2e：stdlib-only 宿主运行时注入 Z42cCompiler → 编 hello → app.zpkg on z42vm。
- self-host 7/7 不回归。

## Deferred
- 按接口批量发现（`ResolveAll(provides)`）；组件版本/兼容协商；参数化 CreateInstance。
