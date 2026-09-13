# Tasks: runtime-ambiguous-use-site

> 状态：🟢 已完成 | 创建：2026-09-13 | 类型：fix（runtime）

**变更说明：** 两个已加载的 zpkg 声明同一个 FQ 名时，运行期此前只 warn 一句、然后**静默跑
加载顺序靠前的那份**。现在**在使用位**抛可捕获的 `Std.MissingSymbolException`。
收口 `harden-crosspkg-gates` 留下的最后一条 Deferred。

## 为什么必须在「使用位」而不是加载时

加载是**惰性且按包**的：两个包完全可能在程序从不触碰的名字上冲突（A 因 `A.X` 被加载、
B 因 `B.Y` 被加载，而它们碰巧都有 `Ns.W`）。加载时报错 = **为一个程序既没引用、也无权修的
冲突把它打死** —— 这正是编译期 E0601 刻意用使用位原则避开的行为，运行期不该反着来。
（上一轮我先按「加载即 error」实现过一版，被实测否掉，见 archive/2026-09-13-harden-crosspkg-gates。）

## 为什么不是「让歧义名解析失败」

好几个调用方把 `try_lookup_*` 的 `None` 当**良性**信号：

| 调用点 | `None` 的含义 |
|---|---|
| `interp::obj_new`（ctor 查找） | 「这个类没有构造函数」——合法 |
| `vcall_resolve` / `dispatch` | 候选链回退（`.or_else(...)`） |

让解析失败会把「歧义」变成「**静默跳过构造函数**」，比现状更坏。且注册表是 **append-only**
（`registry_fingerprint` 的负缓存靠「只增不减」当指纹），移除条目会破坏那个不变量。

⇒ **解析原样不动**；歧义只是被**记下来**，判定挂在真正会派发、且能报错的位置。

## 实现

- **记录**：`LazyLoader.ambiguous: Option<Box<AmbiguousSymbols>>`，在两处 duplicate 分支登记。
  沿用 `negative` 缓存的惯例——**碰撞时才分配**，没碰撞的程序零代价；append-only。
- **快门**：进程级 `AMBIGUITY_SEEN: AtomicBool`。派发路径先读它，进程内从没碰撞过时恒 false
  ⇒ 常态只付一次 relaxed load，**不碰 loader 锁**。（进程级而非 per-loader：它守的注册表
  本来就是进程级的，同 `tokens::alloc_type_id_block` 的理由。）
- **判定落点**：`vm_context::symres` —— 该模块本来就是「派发点的符号完整性判定」之家
  （已有 `missing_type` / `missing_base` / `wrong_ctor_arity` 同族），且用
  `make_missing_symbol_exception` 抛**可捕获**异常而非硬崩。
- **两个后端都接**：interp（`exec_call` 函数位 + `exec_object` 类型位）与 JIT
  （`helpers/call.rs` + `helpers/object.rs`）。**只做一边比不做更糟**——会变成
  「解释执行报错、JIT 静默跑错的那份」。

## 验证

- [x] 真实场景（**编译期永远看不到的那种冲突**）：app 依赖 a+b，编译时只有 a 提供
      `Demo.Shared.W`；随后「b 升级」新增同 FQN 的 `W`，**app 不重编**直接跑。
      修前：静默打印 `from-b`（app 是照 a 编的！）；修后：两个模式都抛
      `Std.MissingSymbolException`（interp 还带源码位置 `Main.z42:8:5`）。
- [x] 负例：把冲突移除 → 两个模式都恢复 `from-a`。
- [x] 非歧义类型（`Demo.OnlyB.Bee`）在同一次运行里正常放行 —— 判据是精确 FQ 名。
- [x] Rust 单测 3 条：没碰撞不分配 / 记录只标中该名（函数集与类型集互不串、不误伤邻近名）
      / append-only 且幂等。
- [x] **退回对照**：撤掉两处 `note_ambiguous_*` → 两个模式都退回静默 `from-b`。
- [x] `xtask test` 冷构建**全绿** + 自举不动点 3/3
- [x] 零格式 bump

## ⚠️ 本轮踩的坑

**在 `src/runtime` 目录里跑 `cargo build` 会把产物落到别处**（`.cargo/config.toml` 的
`target-dir = "artifacts/build/runtime"` 是相对路径）⇒ 我连续几轮都在用**旧 VM** 验证，
一度以为检查没生效。用 `xtask build runtime` 才对。

**默认执行模式是 JIT**：我先只做了 interp 侧，探针一直不打印才发现——
`z42vm <zpkg>` 不带 `--mode` 时走 JIT。任何「在派发点加判定」的改动都必须两个后端同时做。
