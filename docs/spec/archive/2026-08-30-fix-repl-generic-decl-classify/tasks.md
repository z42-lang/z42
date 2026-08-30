# Tasks: fix-repl-generic-decl-classify

> 状态：🟢 已完成 | 创建：2026-08-30 | 完成：2026-08-30

**变更说明：** REPL 输入分类器只认单 token 类型名的变量/函数声明，泛型 `List<int> a = new()`、
限定名 `Std.Collections.List<int> b = new()`、数组 `int[] arr = new int[3]` 等多 token 类型全部
漏判 → 落表达式/语句路径编成 `Eval{N}` 内局部变量（不入会话），下一轮 `a` 未定义；限定名更被误
解析成比较链（E0401/E0437）。

**原因：** `Classifier.Classify` 用固定 token 偏移判 `<type><ident>=`（token2 必须是 `=`），泛型
`<`、限定名 `.` 一出现即漏判。根因修复 = 引入 `_typeRefEnd` 跳过一个完整类型引用前缀（限定名 +
泛型 + 数组 + 可空），再判其后 `<ident> =`（var）/ `<ident> (`（自由函数）。

**文档影响：** `src/libraries/z42.scripting/README.md`（Classifier 行）+ `docs/design/toolchain/repl.md`
（输入分类表 + 类型前缀跳过说明）。

- [x] 1.1 `Classifier.z42`：新增 `_typeRefEnd`（限定名/泛型`<>`含嵌套`>>`/数组`[]`/可空`?`）+ `_fillVarDecl` helper；var/函数判定改用类型引用跳过；`DeclType` 存完整类型文本（span 切片）
- [x] 1.2 回归测试 `tests/repl_generic_decl/`（driver.z42 + expected_output.txt）——泛型/限定名/数组/嵌套泛型声明跨轮持久 + 比较表达式不误判护栏；旧分类器该测试 `E0401` 全红，新分类器全绿
- [x] 1.3 文档同步：scripting README Classifier 行 + repl.md 分类表/类型前缀跳过节

## 验证

- e2e（warm-z42c 回路，SDK z42c 重建 z42.scripting 换入 libs 跑 z42i）：
  `List<int> a = new()` → 绑定，`a.Add(5)`/`a.Count`→2；`Std.Collections.List<int> b`、`int[] arr`、
  `Dictionary<string,int> d` 全部绑定持久；`p < 5`→true（比较不误判）；`var v = ..` 仍走推断。
- 驱动测试 `repl_generic_decl` 实际输出逐行对账 expected_output.txt（16 行全绿）；旧 scripting 同测试首行即 `ok`→`E0401` 发散。
- **注**：scripting 驱动测试（`tests/<name>/driver.z42`）非 CI 门禁自动跑（现状约定，同 `repl_target_typed_new`），
  为可手动复跑的 canonical 回归；本地 e2e 已验。纯 stdlib scripting 改动、零格式 bump。
