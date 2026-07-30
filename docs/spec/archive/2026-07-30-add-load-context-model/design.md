# Design: 加载上下文模型（LoadContext / ALC 地基）

> 上位设计：[load-context.md](../../../design/runtime/load-context.md)（ALC 全景，本 change 落其 §3 上下文模型的 Phase 1 地基）。
> 本 change **不含**卸载/回收/诊断——那些是后续 change。

## Architecture

```
                       VmCore
                         │
                 ContextRegistry
                 ┌───────┴────────────────────────────┐
                 ▼                                     ▼
        ┌──────────────────┐              ┌──────────────────────────┐
        │ root context     │              │ collectible context "A"  │
        │ ContextId(0)     │              │ ContextId(n)             │
        │ IsCollectible=F  │              │ IsCollectible=T          │
        │                  │              │                          │
        │ 现有扁平 Module   │◀────元数据────│ 独立 arena（本 change     │
        │ Vec<Function>    │  引用(base=   │ 用一个 owned Module 承载） │
        │ type_registry    │  Object 等)   │ 载入的 zpkg → 反射可见     │
        │ O(1) MethodId    │              │ TypeDesc.ctx = A          │
        └──────────────────┘              └──────────────────────────┘
                 ▲                                     │
                 │  Assembly 反射投影（native 句柄）      │
      typeof(T).Assembly ───────────────────────────────┘
      typeof(T).IsCollectible == (T 所在 ctx).IsCollectible
```

- **root** 复用现有 `merge_modules` 扁平 `Module` + `MethodId` 位置索引 dispatch，**一字不改**——
  99% 代码在 root，热路径零回归。
- **collectible** 各持一个独立 arena（Phase 1 用一个 context-owned `Module` 结构承载载入的 zpkg），
  其 `TypeDesc` 回指所属 `ContextId`。
- **Assembly** = zpkg 的运行时反射投影，native 句柄背书（与 `Type` 同构，`NativeData` 新变体）。

## Decisions

### D1: root 保持扁平 merge，collectible 独立 arena（不 de-merge root）
**问题：** 引入上下文边界，是否要把现有扁平 `Module` 按 zpkg 拆开？
**选项：** A—全 de-merge，每 zpkg 独立 arena；B—root 保持扁平，仅 collectible 独立。
**决定：** 选 B。de-merge 会让跨 zpkg 引用铺满间接、退化现有 O(1) MethodId dispatch（热路径回归）。
root 永驻不回收，无需边界；只有"想卸载/重载"的代码才付独立 arena + 跨界间接的代价。这就是
"粒度比 dotnet 更小"的落点——**上下文单元大小由用户决定**（root 一大块 + 任意多个可细到单 zpkg
的 collectible），而非 dotnet 固定死在 assembly。

### D2: Assembly 作 native-handle 反射类型（FC1=A）
**问题：** z42 无 `Assembly` 类型，`IsCollectible` 该挂哪？
**决定：** 新增 `Std.Reflection.Assembly`，native 句柄背书，仿 `Type`（`NativeData::AssemblyHandle`）。
理由：Phase 1 命题就是"运行时保留 zpkg 身份"，Assembly 正是这身份的反射投影；`Type.Assembly.IsCollectible`
与 .NET 对齐；给 `IsCollectible`/`LoadContext` 干净的家。`Type` 加 `Assembly` 属性接上链路。

### D3: Unload() 声明但抛 NotSupportedException（FC2=(ii)）
**问题：** Phase 1 不做回收机制，`Unload()` API 出不出现？
**决定：** 声明 `Unload()`，Phase 1 抛 `NotSupportedException`，回收机制下一 change 落。
**权衡（philosophy 事实校正）：** 这引入一个"声明了但会抛"的 API，轻微违反"不搞临时方案"。
但 User 已明确接受"先加 API 形态、下一阶段深度迭代"的节奏（FC2=(ii)/FC3 同调），且它诚实标记
"此上下文日后可回收"的意图——与 .NET `IsCollectible=true` 在 unload 前就为真同理。**message 必须
明确指向后续 change**，不得让调用方误以为静默成功。

### D4: LoadZpkg/CallStatic stub 不动，Default 路径保兼容（FC3）
**问题：** 既有 `Std.Runtime.Runtime.LoadZpkg/CallStatic`（DEFERRED stub）怎么处理？
**决定：** Phase 1 不动。`LoadContext.Load` 是动态加载能力的正确设计归宿（context-scoped），
等本 change 落地后单开小 change 删 stub（pre-1.0 无兼容负担）。Phase 1 一切加载默认经 root/Default
（现有 merge 路径），**确保零回归**。

### D5: context/assembly 关联放注册表 + Type 对象 `__asmId` 槽（不 mutate TypeDesc）
**问题：** `Type.IsCollectible` / `Type.Assembly` 如何解析？
**选项：** A—给 `TypeDesc` 加 `context` 字段；B—关联放 `ContextRegistry`（context→assembly→module）
+ 在 `Std.Type` 对象上存一个 `__asmId` 隐藏槽。
**决定：** 选 B。**实施期事实校正**：`TypeDesc` 只 derive `Debug`（非 `Clone`），且 `load_artifact`
内部就把 TypeDesc 以 `Arc` 别名进 `type_registry` + `type_registry_vec`，加载后再 mutate/rebuild
其 `context` 字段代价大、且要改 loader build 路径。B 方案零改 build 路径：
- 关联全在 `ContextRegistry`（root=ContextId(0)/AssemblyId(0) 预置；`load_into` 把 `Module` 存进
  `AssemblyEntry`）。
- `Assembly.GetTypes()` 用 `make_type_object` 建 `Std.Type` 后 stamp `__asmId` 槽；`typeof(T)` /
  `obj.GetType()` 不 stamp（→ Null → root）。
- `__type_is_collectible` / `__type_assembly` 读 `__asmId` → 注册表查 context 可回收性 / 建 Assembly。
  Null/0 → root → false。**观测行为与 spec 完全一致**，且 TypeDesc 保持不可变（无锁读契约不破）。

### D6: type identity = (context, type) 的地基（面向未来版本共存）
**问题：** 同名类型跨上下文如何区分？
**决定：** Phase 1 把类型→assembly→context 的关联落在 `ContextRegistry` + Type 对象 `__asmId`，
为将来"新旧版本上下文的同名类型是不同类型"（load-context.md §3 / hot-reload 版本共存）留出可达路径
（Type → asmId → context）。Phase 1 不实现版本共存判定；后续若需 per-TypeDesc context，可在收敛
loader build 路径时补，届时的破坏面用同一迭代清零（pre-1.0 无兼容负担）。

### D7: 静态成员用 extern 方法，实例 getter 用 extern 属性（语言能力约束）
**问题：** `Default` / `CreateCollectible` 用静态属性还是静态方法？
**实施期事实校正：** z42 stdlib **全库无静态属性**先例（`Std.GC` 等静态成员一律 extern 方法），
静态 extern 属性在编译器/语言中未验证。**决定：** 静态成员（`Default()` / `CreateCollectible()`）
落为**静态 extern 方法**，实例 getter（`Name` / `IsCollectible` / `Assembly` / `LoadContext`）落为
**实例 extern 属性**（proven）。与现有 stdlib 完全一致。API 由 `LoadContext.Default` 微调为
`LoadContext.Default()`——.NET 的 property 语义在 z42 当前能力下以方法承载。

## Implementation Notes

- **绑定范式**（照抄 `Std.GC`）：z42 `[Native("__snake")] extern` → Rust
  `builtin_<name>(ctx, args) -> Result<Value>` 于 `corelib/loadcontext.rs` → `corelib/mod.rs`
  的 `BUILTINS` 表按序注册（顺序即 `BuiltinId`，追加到表尾，勿插中间以免扰动既有 id）。
- **Value 句柄**：`NativeData`（`metadata/types.rs`）加 `LoadContextHandle(ContextId)` +
  `AssemblyHandle(...)` 变体；`LoadContext` / `Assembly` z42 实例经 `NativeData` 携句柄，与
  `Type` 的 `TypeHandle` 同构，不可用户构造。
- **ContextRegistry**：`VmCore` 持 `context_registry: ContextRegistry`（root 于 VM 初始化即建，
  ContextId(0)）。`CreateCollectible` 分配新 ContextId + 空 arena。线程安全按现有 `VmCore` 锁范式
  （`Mutex`/`RwLock`，参照 `static_fields` / `lazy_loader`）。
- **加载分叉**（`metadata/loader.rs`）：现有入口 → root（`merge_modules`，不动）；新
  `load_into_context(ctx, path)` → 解析 zpkg + 建 `Module`/`TypeDesc`（`context=ctx`）存入该 ctx
  的 arena，**不** merge 进 root。跨界类型引用（base=Object 等）向 root 解析（只读）。
- **collectible 执行 = 本 change Out of Scope**：Phase 1 载入的 zpkg 只保证**反射可见**，其函数
  能否跨界调用不在本 change（下一步）。测试用例只反射、不调用。
- **文件行数**：`loadcontext.rs` 控制在 300 软限内；超则按 builtin 组拆子模块。

## Testing Strategy

- **Rust 单测**（`corelib/loadcontext_tests.rs`）：ContextRegistry root=ContextId(0)/IsCollectible=false；
  CreateCollectible 分配新 id/IsCollectible=true；`__type_is_collectible` 对 root TypeDesc 返 false；
  `Unload` builtin 返/抛 NotSupported。
- **e2e golden**（`src/tests/load-context/collectible-reflection/`）：一个小 dep zpkg（源在
  `dep/`，e2e 框架预编）→ 主程序 `CreateCollectible` → `Load` → `GetAssemblies`/`GetTypes` →
  断言 `IsCollectible==true` + `t.Assembly==asm` + `asm.LoadContext==ctx`；对比
  `typeof(int).IsCollectible==false`；`ctx.Unload()` catch `NotSupportedException`。
- **兼容回归**：`xtask test`（完整 GREEN gate）——root 路径逐字节不变由 e2e/stdlib/compiler
  全绿 + 自举 gen1==gen2 保证。
- **VM 验证**：完整 `xtask test`（e2e + cross-zpkg + stdlib + compiler + vscode-syntax）。
