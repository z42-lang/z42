# Design: 接口成员枚举

## Architecture

```
z42c 编译期                                    运行期
─────────                                     ─────
接口声明 IShape { double Area(); ... }
   │ IrGen（迭代接口成员，NEW）
   ├─► 每方法：_emitAbstractStub（复用！）──► SIGS/FUNC
   │      method_flags = abstract|virtual          │ resolve_func_sig
   │      + visibility + param 元数据               ▼
   └─► ClassDescBuilder._interfaceDesc（改）──► TYPE 段接口条目
          方法表（qualified → vtable/own_methods）    │ loader → TypeDesc.vtable/own_methods
                                                     ▼
                                       builtin_type_methods（不改）
                                       typeof(IShape).GetMethods() → [Area, Scale]
```

## Decisions

### Decision 1: 复用 abstract 方法的签名桩路径（不新建机制）
**问题**：接口方法无 body，如何进入反射可见的方法表？
**选项**：A 新建"接口方法"专用 wire 块；B 复用 abstract 方法的 `_emitAbstractStub`（body-less 死体桩，进 SIGS/FUNC + method_flags abstract）。
**决定**：选 B。接口方法与 class abstract 方法**同形**（无 body、隐式 abstract+virtual、override 经 vtable 派发、桩永不被调）。`add-method-modifiers`（unify P1-c）已为 class abstract 方法铺好这条路（IrGen.z42:195 段）——接口方法直接走同一发射器。零新机制、最小改动面、天然带对齐的 IsAbstract/IsVirtual/params 元数据。

### Decision 2: 接口方法归 vtable（virtual），对齐 class virtual 呈现
**问题**：`builtin_type_methods` 读 vtable（virtual/继承，`(simple,qualified)`）+ own_methods（非虚，qualified）。接口方法放哪？
**决定**：vtable。接口方法隐式 virtual，与 class virtual 方法呈现一致（`build_method_info(is_virtual=true)`）。ClassDescBuilder 为接口填 vtable 槽（simple→qualified）。

### Decision 3: 只发直接声明方法（继承接口方法延后）
**问题**：`IBar : IFoo` 时 IBar.GetMethods 是否含 IFoo 方法？
**决定**：只含 IBar 直接声明（对齐 C# 默认）。基接口方法用户经 `GetInterfaces()` 各自取。传递闭包 = Deferred（避免 MVP 引入方法去重/T-替换复杂度；接口传递闭包在 GetInterfaces 已有先例，方法闭包留后续）。

### Decision 4: version bump —— 实现期以事实判定
**问题**：填接口方法块是否需 format bump？
**分析**：TYPE 段 `BuildType`（ZbcWriter:245+）**逐类逐方法**写 MethodFlags/visibility/params。若接口条目走同一 `BuildType` 路径（方法块 count 当前为 0），填非空块 = **同结构、reader 不变 → 无 bump**（似 add-reflection-properties 的"运行期派生、零格式变更"精神）。若接口条目走**独立截断序列化**（整块省略），则 bump zbc 1.27→1.28 / zpkg 0.32→0.33，按 [version-bumping.md](../../../.claude/rules/version-bumping.md) checklist 同步 writer/reader/strict-pin/fixture golden。
**决定**：实现第一步先**勘察 `_interfaceDesc` → BuildType 的序列化路径**证实/证伪"复用现块"，据此决定是否 bump——写进 tasks 阶段 1.0。

## Implementation Notes

- **发射序稳定**：接口成员按**声明序**迭代 emit（同 class 方法两遍法，byte-identical 依赖），保 gen1==gen2。
- **qualified 名**：`<Interface FQ>.<Method>`（同 class 方法 key 规则；stabilize-dispatch-keys 方案A 全签名 mangle → key 带 `$N$types`，反射 `Name` 已 strip mangle 后缀，见 `build_method_info`）。
- **z42c 自身含接口**：z42c 源码有接口声明；本变更会改 z42c 自己的 zpkg 字节（接口条目变大）→ golden/fixture regen；自举不动点靠确定性发射保持。
- **协议豁免名**：接口不声明 ToString/Equals 等 Object 协议方法，无需特判。

## Testing Strategy

- **stdlib [Test]**（reflection.z42）：新增 `IShape`（`double Area()` / `void Scale(double)`）+ `IBar : IFoo` → 断言 GetMethods 含声明方法、签名/参数/IsAbstract/IsVirtual、GetMembers 含之、派生接口不含基接口方法。
- **GREEN gate**：`./xtask test` 全绿——重点 **compiler 自举不动点 7/7**（发射改动最敏感面）+ e2e + stdlib。
- **format**：若 Q1 判定 bump，追加 fixture 字节 golden 更新 + strict-pin 版本断言（zbc_reader_tests.rs 的 27→28 / 32→33）。
