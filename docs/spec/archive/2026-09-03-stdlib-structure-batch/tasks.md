# Tasks: 标准库结构批次（stdlib-structure-batch）

> 状态：🟢 已完成 | 创建：2026-09-03 | 完成：2026-09-03 | 类型：refactor + test（stdlib + runtime 测试）
**变更说明：** ① `List<T>` 去掉类级 `where T: IEquatable<T> + IComparable<T>` 约束（两个 partial 文件），对齐 C#
`List<T>`（无约束；`Sort()` / `Contains` 靠 `CompareTo` / `Equals` 的运行期派发，与 C# 一致）；② 新增 Rust 单测
`corelib/native_decl_tests.rs`：扫描 `src/libraries/**/*.z42` 的 `[Native("__x")]` 声明与 `BUILTINS` 双向对账
（未声明的 builtin 须在显式 allowlist 中并写明原因；allowlist 过期/写错也判红）。
**原因：** 三面评审 L-8 / L-7。① stdlib 自身已大量实例化 `List<JsonValue>` 等不实现两接口的类型——编译器目前
只登记接口约束不校验（`ConstraintChecker` 延后 interface 约束），一旦校验落地 stdlib 先崩；② 声明与实现无任何
一致性校验，漂移到加载期才 panic。
**文档影响：** `src/libraries/z42.core/src/Collections/List.z42` 头注释；`src/runtime/src/corelib/README.md`
（测试段：加 builtin 需同时声明或进 allowlist）。

## 进度概览
- [x] 1. List / List.Query 去约束 + 注释同步
- [x] 2. native_decl_tests.rs + mod.rs 接线 + corelib README
- [x] 3. 验证：`cargo test --lib native_decl` + `xtask test`
- [x] 4. 归档

## 备注
- 双向对账实测（2026-09-03，main fb99e55b）：z42 声明 290 个名字全部在 BUILTINS；BUILTINS 301 项中 11 项未声明
  → 全部进 allowlist（编译器直接发射 2 / VM 内部 1 / host-only 5 / legacy 3）。

## 验证记录（2026-09-03）
- `cargo test --lib native_decl`：2/2（声明→表、表→声明∪allowlist，含 allowlist 过期/写错检查）。
- `xtask test` ✅ GREEN 14:10（e2e 全过；z42c [Test] 23/23；self-host 不动点 3/3；vscode-syntax）——`List<T>` 去约束后
  stdlib / 编译器 / 全部 golden 无变化。
