# Design: 跨包静态方法调用触发 cctor

## Architecture

```mermaid
flowchart LR
  subgraph 加载
    A[app.rs 合并后全扫<br/>急切类型] -->|register| R[(CctorRegistry<br/>pending 计数)]
    L1[load_zpkg_file] --> I[LazyLoader::insert_type<br/>单一入口]
    L2[load_module 内存模块] --> I
    I -->|register| R
  end
  subgraph 使用（热路径）
    C[静态字段读写 / 静态方法调用] --> G{any_pending?}
    G -- 否 --> X[直接执行]
    G -- 是 --> E[ensure_*_owner_init → claim → 跑 cctor]
  end
  R -.门.-> G
```

改动前，`I` 这一格不存在：惰性类型只在 `try_lookup_type` 登记；跨包静态方法调用走 `try_lookup_function`，
从不经过它 ⇒ 门可能恒 0 ⇒ 屏障被短路。

## Decisions

### D1: 在「类型进入加载器注册表」处登记，而不是在「查找」处
**问题**：登记必须早于使用，惰性加载路径上缺一个可靠的时机。
**选项**：
- A — 加载器插入类型时登记（加载器持有 `Arc<CctorRegistry>`）
- B — 加载器插入时只把 `(类, cctor 函数)` 入队，锁外由 `VmContext` 排空登记（仿 `pending_static_inits`）
- C — `try_lookup_function` 解析到静态函数时反查其所属类型再登记
**决定**：A。
- B 要求**每个**释放加载器写锁的调用点都排空一次队列——`lookup.rs` 里取写锁的地方有十余处，这正是
  `cctor.rs` 注释里警告的「多一处要维护的枚举，就多一处将来会漏的地方」；而且 `try_lookup_*` 有
  `static_init_drain_is_noop` 快路径提前返回，排空很容易被跳过。
- C 把代价放到函数解析路径上，且只修「静态方法」一种形态，别的「只查函数不查类型」的入口照漏。
- A 与主模块路径同口径（「类型可见即登记」），只有两个插入点需要收敛成一个。

### D2: 删掉 `try_lookup_type` 里的登记
A 落地后，加载器注册表里的每个类型要么经插入入口登记过，要么是 `seed_types_for_lookup` 播进来的急切类型
（已由 `app.rs` 登记）。`try_lookup_type` 的两处登记就成了第三、第四个登记点，只剩「让人以为那里是必需的」
这一个作用 ⇒ 删除，登记点收敛为「急切合并后」+「加载器插入入口」两处。`ensure_type_init` 内部的幂等登记保留
（它是屏障自身的兜底，不是一个时机）。

### D3: 锁顺序
新增的嵌套是「加载器写锁 → `CctorRegistry.map` 互斥锁」。反方向不存在：`register` / `claim` / `finish` 只在
持有 map 锁的极短区间内读写 map，**从不**在持锁时访问加载器；跑 cctor 函数时 map 锁已释放。故不会死锁。
实施时在 `CctorRegistry` 上把这条写成注释。

### D4: 代价——已加载但从未使用的 cctor 类型会让门常开
登记即 `pending += 1`，只有 cctor 跑完才减。依赖包被加载、但其中某个带 cctor 的类型从未被用到 ⇒ 门常开 ⇒
此后每次静态字段访问 / 静态方法调用多一次查表。
- 急切路径（主模块 + 急切依赖）**今天就是这个行为**，本变更只是让惰性路径与它一致。
- `src/libraries` 与 `src/compiler` 里**没有任何静态构造器**（grep 为 0）⇒ 标准库与编译器不受影响，代价只落在
  真正写了静态 ctor 的用户代码上。
- 不引入「按类型的门」：那需要热路径在门前先拿到所属类型，代价比现在的一次 relaxed load 大得多。

### D5: 静态调用屏障必须在被调函数解析之后（实施中发现）
只做 D1 时实测：`Cfg.Get()` 已正确，但主包第一句 `Loud.Ping()` 仍跳过 cctor（直到之后别的类型让门打开）。
原因是两个后端的调用屏障都位于解析**之前**，而首次跨包调用正是在解析里触发加载与登记。
- JIT `jit_call`：屏障挪到三级解析（by-id / IC / by-name）之后、两条去路（本地 `FnEntry` / `cross_zpkg_via_interp`）之前。
- interp `exec_call::call`：原先跨包解析与执行交织在三个分支里；整理为「先得出 `&Function`（本模块 / cross-cell /
  惰性回落，缺失即抛）→ 屏障 → 原生快路径 → 歧义判定 → 执行」，**仍只有一个屏障点**。
热路径代价不变（门仍是一次 relaxed load）；本模块调用不触发任何查找，顺序调整对它无额外成本。

**字段路径无需同样调整**：静态字段名在函数首次执行时预解析，预解析即触发所属包加载（`pending_type_inits`），早于
字段屏障。夹具 `static_ctor_crosspkg_field_first` 守住这一点。

> ⚠️ 测试陷阱（实测踩过）：同一个 `Main` 里若同时直接引用依赖包的静态字段，预解析会**提前**加载依赖包并登记，
> 把「首次使用是静态调用」的 bug 掩盖掉——修前 VM 也会输出正确结果。故两种形态分成两个夹具。

## Implementation Notes

- `VmCore.cctors: CctorRegistry` → `Arc<CctorRegistry>`；`LazyLoader::new` 增加该句柄参数，测试里构造加载器处
  传 `Default`。
- 加载器新增 `fn insert_type(&mut self, name, desc)`：先查重（保留两条路径各自的 warn / debug 文案与
  `note_ambiguous_type` 语义差异——zpkg 路径记歧义、内存模块路径不记），再插入并登记。
- 与 **#649（fix-host-static-init）** 相邻：它把 `app.rs` 的急切登记挪进 `boot.rs`。本变更不碰急切路径，
  文本冲突预计为零；合并前按规则 rebase + 重跑 GREEN。

## Testing Strategy

- 新 cross-zpkg 夹具 `static_ctor_crosspkg_static_call`：`Cfg`（方法体读静态字段）+ `Loud`（方法不碰字段、cctor 打印）
  + `Unused`（从不使用），主包按 `Loud → Cfg → Loud` 顺序，期望 `Loud cctor / 7 / 42 / 7`；修前 VM interp/jit 均红
  （`7 / 0 / Loud cctor / 7`），只修 D1 时仍红（`7 / 42 / Loud cctor / 7`）。
- 守卫夹具 `static_ctor_crosspkg_field_first`（修前修后均绿）。
- 恢复 #643 两个夹具的静态 ctor 写法（修前应红）。
- Rust 单测：加载一个含 `$Cctor` 哨兵类型的模块后 `registered_count` 增加、`pending` 非 0；未使用时 cctor 不执行。
- 两后端：cross-zpkg runner 的默认模式 + 手工 `--mode interp` / `--mode jit` 各跑一次夹具。
- 改 runtime ⇒ 全量 `cargo test`（不只 `--lib`）+ 全量 `xtask test`。
