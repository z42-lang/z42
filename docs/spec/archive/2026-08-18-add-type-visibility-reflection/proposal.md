# add-type-visibility-reflection — 类可见性反射的 stdlib 面（`Type.Visibility` + 6 bool）

> 状态：已归档（2026-08-18）。SoT（设计与机制）：`docs/book/src/compiler/access-control.md` ① 节。
> 前身 Deferred：`access-future-type-visibility-reflection-surface`（complete-class-access-control ①）。

## What / Why

complete-class-access-control（#186）把类声明可见性字节（zbc 1.33，`0=public/1=private/2=protected/3=internal`）
存入 `TypeDesc.visibility`，并加了 **VM 面 6 个 bool builtin**（`__type_is_public` 族）+ Rust 单测——但按
bootstrap-seed 纪律（#186 同时 bump 格式 → CI 冷启动两代自举用旧 nightly VM，旧 VM 无新 builtin → stdlib 一
引用即 load 期 panic），**stdlib 面推迟一 nightly**。本 change 在 #186 的 nightly 发布后补上 stdlib 反射面。

## 设计决策（User 裁决，2026-08-14；两-PR 落地）

目标：把 `Type` 反射的可见性面**完全对齐 C#**——C# 的 `IsPublic` / `IsNotPublic` /
`IsNested{Public,Private,Family,Assembly}` 是在 `Type.Attributes` flags 之上派生的 **6 个 bool 计算属性**。
z42 也要给这 6 个 bool 属性，而 native interop 只用一个。

- **VM**：单个 `__type_visibility(typeObj) -> int`，返回声明可见性字节
  （handle-less 基元/数组 → `0=Public`，与 C# 一致）。#186 support 期铺的 6 个 bool builtin 收敛成这 1 条。
- **stdlib enum**：`z42.core` 新增 `enum TypeVisibility { Public, Private, Protected, Internal }`；
  `Type.z42` 以 `public extern TypeVisibility Visibility { get; }` 暴露。顶层 vs 嵌套由既有
  `Type.IsNested` 给出（正交轴）。
- **stdlib 6 bool**：`Type.z42` 直接提供 C# 那 6 个 bool，每个是**计算属性**，纯脚本层派生于
  `Visibility × IsNested`：
  ```z42
  public bool IsPublic         { get { return this.Visibility == TypeVisibility.Public   && !this.IsNested; } }
  public bool IsNotPublic      { get { return this.Visibility != TypeVisibility.Public   && !this.IsNested; } }
  public bool IsNestedPublic   { get { return this.Visibility == TypeVisibility.Public   &&  this.IsNested; } }
  public bool IsNestedPrivate  { get { return this.Visibility == TypeVisibility.Private  &&  this.IsNested; } }
  public bool IsNestedFamily   { get { return this.Visibility == TypeVisibility.Protected && this.IsNested; } }
  public bool IsNestedAssembly { get { return this.Visibility == TypeVisibility.Internal &&  this.IsNested; } }
  ```
  interop 面仍是 1 个 builtin，API 与 C# 逐一对齐。

**两阶段两 nightly（bootstrap-seed 纪律）**：命名属性的计算 getter 此前 z42 不支持（`Name { get { return
… } }`，仅 indexer 支持），种子 z42c 无 support 时 stdlib 一写计算属性即冷启动硬 parse 报错（实测）。故拆两 PR：

1. **PR1 = add-property-getter（#220，已合并 main 7ee6fab5）**：给 z42c 补命名属性计算 getter 语言特性
   （compiler-only，复用 indexer body-getter 流水线）。
2. **PR2 = 本 change**：待含 #220 的 nightly 发布后，`Type.z42` 用计算属性写这 6 个 bool。种子 z42c 已含
   support（本 PR 用当前 nightly 供种 + 冷建 stdlib 24/24 通过验证）→ 无冷启动 parse 报错。

**无格式 bump**（可见性字节 #186 已在线；6 bool 编译为 `get_IsPublic` 族普通方法，无新 IR/格式）→ 无两代
自举。`__type_visibility` builtin 不在旧 nightly seed，但 warm/CI 自建路径均用 cargo release VM 加载 stdlib
（非 seed VM），故安全。

## 改动面

- `src/runtime/src/corelib/reflection.rs`：删 6 bool builtin + `type_is_nested_name` helper，加 `builtin_type_visibility`。
- `src/runtime/src/corelib/mod.rs`：BUILTINS 表 6 条 → 1 条（`__type_visibility`）。BuiltinId 按名加载解析，收敛安全。
- `src/runtime/src/corelib/reflection_tests.rs`：2 个单测改测 `builtin_type_visibility` + `builtin_type_is_nested`。
- `src/libraries/z42.core/src/TypeVisibility.z42`：新 enum。
- `src/libraries/z42.core/src/Type.z42`：`Visibility` extern 属性 + 6 个 bool 计算属性（对齐 C#）。
- `src/tests/types/type_visibility.z42`：golden（顶层 public/internal + 嵌套四级 + 基元；6 bool + Visibility enum）。
- `docs/book/src/compiler/access-control.md`：① 节改为已实现（enum + 6 bool 计算属性）+ Deferred 条目消。

## 验证

- `cargo test --lib`（2 个 visibility 单测）。
- `xtask test`（新 golden type_visibility）全绿 + 自举 5/5 gen1==gen2。
- 无格式 bump、跨包无关。
