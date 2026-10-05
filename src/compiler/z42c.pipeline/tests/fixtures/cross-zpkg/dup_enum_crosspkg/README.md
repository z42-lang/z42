# dup_enum_crosspkg — 跨包同 FQN 的 enum（E0601）

两个互不依赖的包各声明 `Demo.EnumNs.Color`，**成员顺序相反**。

## 为什么单独立一道门

- **这是本族里唯一「静默错值」的形态**：其它冲突错的是「绑到哪一份类型」，enum 错的是
  「算出什么数」——`EnumConsts` 是**裸键**（`Enum.Member`）⇒ first-wins ⇒ `Color.Green` 一边是
  1、一边是 0，编译期若无诊断则零提示。
- **单测覆盖不到真实那条路**：`ExportedTypeExtractor.Extract` 从不抽取源码里的 enum
  （硬编码 `BuiltinTypeDefs._builtinEnums()`），真实跨包 enum 由 `TsigReconcile` 从 zbc TYPE 段
  重建。单测里只能手工构造 `ExportedEnumZ`（见 `crosspkg_duplicate_tests.z42`），
  **TSIG 这条路只有本 fixture 守着**。

## 实现要点

`_mergeImportedEnums` 并入 `EnumTypeNs`，使导入 enum 的 `Z42ClassType.Enum(name)` 带上命名空间、`Fqn()` 与
`ClassPkgAll` 的 `ns.Name` 对得上，类型注解位的既有检查据此生效；常量读那条另由 `ChkEnumOrigins` 覆盖
（它绑成 `BoundLitInt`，不经任何类型引用检查）。

负例 fixture 约定（`expected_build_error.txt`）见 `../README.md`。
