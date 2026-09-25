# Tasks: 类级 `typeof(T)` 产出真实类型

> 状态：🟢 已完成 | 创建：2026-09-25 | 完成：2026-09-25
> 分支 `fix-class-level-typeof` @ worktree `../wt-clstypeof`（起点 main `47dc5e1a7`）

## 进度概览
- [x] 阶段 1: 运行期 builtin
- [x] 阶段 2: 编译期绑定 + 发射
- [x] 阶段 3: 测试
- [x] 阶段 4: 验证与文档

## 阶段 1: 运行期 builtin
- [x] 1.1 `reflection/generics.rs` 新增 `builtin_class_type_arg(ctx, args)`：
      `args[0]` receiver → `type_args()[idx]` → `make_type_from_name`；
      非对象 / 越界 / 空 → `make_constructed_type(ctx, "T", &[])`
- [x] 1.2 ➖ **无需改动**：`reflection/mod.rs` 已有 `pub use self::generics::*;`，新 `pub fn` 自动导出
- [x] 1.3 `builtin_table_ext.rs` **PART2 表尾追加** `("__class_type_arg", reflection::builtin_class_type_arg)`
      + 按既有格式写 `appended to preserve existing BuiltinIds` 注释
- [x] 1.4 `cargo build --manifest-path src/runtime/Cargo.toml --release` 通过

## 阶段 2: 编译期绑定 + 发射
- [x] 2.1 `BoundExprOp.z42`：`BoundTypeof` 增 `IsClassLevel` / `ClassParamIndex`（ctor 里初始化
      为 `false` / `-1`）；订正抬头的 D3 注释（不再说「类级仍产占位」）
- [x] 2.2 `TypeOpTyper._bindTypeofExpr`：方法级分支之后补类级分支，判据
      `ClassParamIndexOf(name) >= 0 && env.LookupVar("this") != null`（Decision 2）
- [x] 2.3 `TypeOpEmitter._emitTypeof`：类级分支发 `ConstI32 + BuiltinInstr __class_type_arg(this, idx)`；
      `Locals.Get("this")` 为 null 时防御性落回占位路径
- [x] 2.4 `xtask build sdk` 重建（🔴 产物在 `artifacts/.z42/`，**不是** `.z42/`）
- [x] 2.5 探针实跑：`./artifacts/.z42/z42 run <probe>.z42` 确认 `Std.Int32` 已出现
      （⚠️ 用 `./.z42/z42` 会得到一字未变的假阴性）

## 阶段 3: 测试
- [x] 3.1 `src/tests/generics/class_level_typeof.z42` —— 正面 7 条（spec 的 MODIFIED 全场景）
- [x] 3.2 `src/tests/generics/class_level_typeof_edges.z42` —— 边界 4 条
      （含 Decision 2 的钉子：静态语境带对象首参**必须**仍产占位）
- [x] 3.3 `reflection_tests.rs` —— builtin 三条降级路径的 Rust 单测
- [x] 3.4 `examples/types/generics/gaps/typeofgap.z42` 注释与期望值订正
- [x] 3.5 `examples/types/generics/gaps/run.console` transcript 重放（**输出以实跑为准**）
- [x] 3.6 **阴性对照**：退回 2.2 的类级标记 + 重建 SDK → 两个用例都判红（interp + jit 各 2 条）。
      ⚠️ **`edges` 也红，原因要说清**：它末尾那条正面断言（`b.full() == "Std.Int32"`）是我
      刻意放的「判据没把正面路径一起关掉」的钉子 —— 阴性对照下它当然红。**四条边界断言本身
      一条没变**：同一构建下逐条打值实测得 `T` / `T` / `T` / `Std.String`（探针 `t3e.z42`），
      即降级路径与遮蔽护栏确实未被改动。⇒ 两侧都有判别力，不是恒不响的门。

## 阶段 4: 验证与文档
- [x] 4.1 `xtask test` 完整 GREEN（改了编译器 ⇒ 先 `xtask build sdk`）
- [x] 4.2 `xtask test compiler` 字节不动点无漂移（**实跑确认，不靠推理**）
- [x] 4.3 `docs/reference/src/language/generic-methods.md`：删「🔴 类级产占位名」段，
      改为「两级都具化」+ 保留的两条边界（静态语境 / 继承基类）
- [x] 4.4 `docs/learn/src/types/generics.md`：第 18 章坑点段（241-256）与小结「四个边界」订正
- [x] 4.5 `docs/internals/src/compiler/generics.md`：补机制段（两个载体 + 为何用 builtin 而非新 opcode）
- [x] 4.6 `docs/roadmap.md`：D3 延后项消化（若 roadmap 有索引条目）
- [x] 4.7 spec scenarios 逐条覆盖对账
- [x] 4.8 归档：tasks.md 改 🟢 + `changes/` → `archive/2026-09-25-fix-class-level-typeof/`
      （🔴 **必须在开 PR 之前 commit 进本分支**，阶段 9 铁律）

## 备注

### 🔴 顺带发现、不在本 Scope 的静默 bug（记入，不顺手修）

**静态语境的 `default(T)` 会读第一个实参的 type_args。** 实测：

```z42
class Box<T> { public static string Peek(Box<int> o) { T z = default(T); return "[" + z + "]"; } }
Box.Peek(new Box<int>())   // 实测 [0]，应为 [null]
```

根因：`default_of` 读 `frame.get(0)`，静态帧 reg0 = 第一个实参而非 `this`
（`exec_address.rs:78-88`）。归第二刀（继承链寻址）一并修——那一刀本就要改这个载体的寻址口径。
本刀的 Decision 2 判据保证**不把该隐患搬进 `typeof`**。

### 📌 顺带订正的文档漂移（同一 Scope 文件内，非扩张）

`docs/learn/src/types/generics.md` 小结那句「记住**四个**边界」里有**两条早已修好**、正文已改而
小结没跟上：`new T()` 遇基元会崩（#803 修）、泛型 ctor 实参不检查（#812 修）。两条均**实跑复核**
（`new int()`→`0`、`new bool()`→`false`、ctor 实参→`E0402`）后改成「三个边界」。
我要改的正是同一句（要删掉「类级 `typeof(T)` 给不出真类型」），留着错的等于知情交付错文档。
⭐ 同族于催生 B11 的那条教训：**正文有门禁盯着、小结没有**。

### 环境（新会话照抄）

- worktree `../wt-clstypeof`，分支 `fix-class-level-typeof`，已供种 + xtask 已重建
- ⚠️ `RUSTUP_TOOLCHAIN=1.98.1`；Rust workspace 在 `src/runtime/`，不在仓库根
- 🔴 验修复一律 `./artifacts/.z42/z42 run`（`xtask build sdk` 的产物）；`./.z42/z42` 是种子
- ⚠️ `xtask build all` 全红也返回 exit 0 ⇒ 必须 grep `✗` / `failed`
- 探针在 `<scratchpad>/gaps/`：`t3.z42`（主）/ `t3s.z42`(静态) / `t3h.z42`(reg0 隐患) /
  `t3g.z42`(泛型派生撞车) / `t3c.z42`(GetType 路线) / `t3x.z42`(遮蔽+数组) / `t3y.z42`(数组串)
