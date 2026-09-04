# Design: 类初始化按需触发

## 现状机制

```
Vm::run → interp::run_with_static_init → init_static_fields(ctx, module)
   ① 跑主模块内所有 *.__static_init__（module.functions 过滤，按名排序）
   ② ctx.collect_lazy_static_init_names()
        └─ LazyLoader::force_load_all_declared()   ← 把所有候选包整包加载
        └─ 对 function_table 全表扫后缀 ".__static_init__"
      逐个 try_lookup_function + exec_function
```

JIT 侧 [`jit/mod.rs:256`](../../../../src/runtime/src/jit/mod.rs) 是同一逻辑的镜像。

**为什么必须提前跑**：`VmContext::static_get` 读到未初始化的字段直接返回 `Value::Null`
（[statics.rs:45](../../../../src/runtime/src/vm_context/statics.rs)），没有任何「触发所属类初始化」的机制。
`loader/namespace.rs` 末尾留有 2026-04-27 `fix-static-field-access` 的事故记录：
「不 lazy-load z42.math → `__static_init__` 不跑 → 字段永远 null」——当时的解法就是「全部提前加载」。

## 目标机制（CLR / JVM 语义）

一个包的 `__static_init__` 在该包**首次被真正用到**时执行。三个触发点：

| # | 触发点 | 位置 | 说明 |
|---|--------|------|------|
| T1 | 函数查找 | `VmContext::try_lookup_function` | 已有懒加载；补排空队列 |
| T2 | 类型查找 | `VmContext::try_lookup_type` | 已有懒加载；补排空队列 |
| T3 | **静态字段引用** | `metadata::resolver` 解析 `StaticGet`/`StaticSet` 名字时 | **本变更新增，当前架构缺失** |

### T3 为什么放在 resolver 而不是 `static_get`

`static_get_by_id` 是热路径（每次静态字段读一次），且**无法区分「未初始化」与「值就是 null」**——
`Value::Null` 是合法值，没有 miss 信号。而 `metadata::resolver` 在模块解析时
就把每条 `StaticGet/StaticSet` 的字段名解析成 `StaticFieldId`（[resolver.rs:347](../../../../src/runtime/src/metadata/resolver.rs)），
名字在那里是可得的、每个名字只走一次。

因此 T3 = 解析静态字段名时，若其**所属类**（名字去掉最后一段）不在 type registry 中，
把该类 FQN 入队；在模块执行前排空队列，对每个类走 `try_lookup_type` —— 这会触发所属包加载，
进而触发该包的初始化。**热路径 `static_get_by_id` 零改动、零新增开销。**

### 队列与锁

`LazyLoader::load_zpkg_file` 注册函数时，把 `*.__static_init__` 名字压入
`pending_static_inits: Vec<String>`。`VmContext::run_pending_static_inits()` 负责排空：

```
loop {
    取出并清空 pending（持 loader 锁，仅 mem::take）
    若为空 → return
    释放锁
    对每个名字 try_lookup_function + exec_function
}
```

**必须放锁后执行**：初始化器自身会调用其它函数 / 触碰其它类型，从而再次进入
`try_lookup_*` 并需要 loader 锁；持锁执行必死锁。循环是因为初始化器可能又拉进新的包。

### 重入与并发（CLR 的做法）

新增 `init_state: FxHashMap<PkgName, InitState>`，`InitState ∈ {Running(ThreadId), Done}`：

- **重入**（A 的初始化器触发 B，B 的初始化器又触发 A）：同线程再次请求 `Running` 的包 → 立即返回，
  允许观察到部分初始化状态。与 CLR 对循环类型初始化器的处理一致（不死锁、不报错）。
- **并发**（多 `VmContext` 线程同时首次触达同一包）：非持有线程等待 `Done`。
  实现上复用 loader 锁 + condvar；初始化器执行在锁外。
- `static_fields_clear()` 必须同时清空 `init_state`，否则清空后已「Done」的包不会重跑。

### 候选集收窄（P3）

`app::build_declared_candidates` 按 entry 的 `import_namespaces` 反查包。hello 的该列表是
`["Std", "Std.IO", "Std.IO.Console"]`——**根命名空间 `Std` 匹配 11 个包**（每个 stdlib 包的 NSPC 都含 `Std`）。
改为：命名空间列表按长度降序，只对**最具体的那些**路由；根命名空间（单段，如 `Std`）不参与候选路由。

风险：若某程序引用直接位于 `Std` 下的类（如 `Std.Math`），其 IMPT 中会有 `Std.Math` 这一更具体项，
仍能路由。仅当程序引用一个**只声明了 `Std`、没有更具体命名空间**的包时才会漏——已扫描 stdlib，无此情形。
tasks 中列为必查项。

### 原生类型名守卫（P4）

`LazyLoader::resolve_type` 对无点号且属于原生关键字集
（`int/long/short/byte/sbyte/uint/ulong/ushort/float/double/bool/char/string/object/void`）的名字
直接返回 `None`，不进 Fallback-B。这些是编译器关键字，不可能是 zpkg 导出类名。
与 `corelib::reflection::type_query::is_primitive_type_name` 的集合保持一致（后者不含 `string/object/void`，
本处需要更宽的集合，二者用途不同，不合并）。

## REPL 影响（User 指定确认项）

REPL 每一轮走的是**另一条注册路径**，与本变更的队列不相交：

```
用户输入 → z42.scripting 编译成字节 → builtin __load_bytecode_in_memory
  → VmContext::load_module_bytes_into_vm → LazyLoader::load_module_from_bytes
  → register_loaded_artifact()  ← 已有逻辑：收集本轮 *.__static_init__ 并**返回给调用方**
  → 调用方只跑本轮的初始化（注释明示：全量 clear+rerun 会抹掉前几轮改过的静态状态）
```

本变更的 `pending_static_inits` 压入点在 `load_zpkg_file`（懒加载 zpkg 路径），
与 `register_loaded_artifact` 不同函数 → **REPL 轮次的初始化器不会被重复执行**。

四项 REPL 行为需要验证（tasks 阶段 8）：

1. **启动**：`z42i` 自身是 z42 程序，其 boot 同样不再 force-load → REPL 启动更快、更省内存。预期正收益。
2. **跨轮引用新包**：用户输入 `Std.Json...` → 本轮字节码加载 → 调用解析触发 z42.json 加载 → T1 排空 → 初始化。
   首次停顿实测量级 0.10–2.08 ms（各包加载耗时），用户不可感。
3. **跨轮静态字段**：用户输入 `Std.Math.PI`（所属包未加载）——这正是 golden `generic_field_carry` 暴露的路径。
   必须靠 T3 覆盖；REPL 轮次的字节码同样过 `metadata::resolver`，故 T3 自动生效。**必须有专门用例。**
4. **静态状态延续**：REPL 刻意不做全量 clear+rerun。`init_state` 与 `static_fields` 同生命周期，
   不随轮次清空 → 前几轮触发过的包不会被二次初始化、不会覆盖用户改过的静态值。

REPL 未被 `xtask test` 默认覆盖（仅 `xtask test dist` 有一条 `z42 repl -c '1 + 2'` 冒烟），
本变更须补一条多轮 REPL 用例（含静态字段访问），否则回归无网。

## 实施中发现的隐藏耦合：候选集不是传递闭包

**症状**：去掉 boot 期 force-load 后，`xtask test` 立刻炸在
`Std.IO.Process.AppendString` 的 `arr.Length`（`this._args` 是 Null）。

**根因链**（逐环实测）：

1. `LazyLoader::declared_zpkgs` 是**增量**集合：初值来自入口的直接依赖 + IMPT 命名空间，
   每加载一个包才把**它的**依赖注册成新候选。
2. xtask 的直接依赖是 cli / crypto / encoding / json / project / regex / text / toml 八个；
   **z42.io 只是 z42.cli 的传递依赖**，一开始不在候选集里。
3. `new Process("git")` → `resolve_type("Std.IO.Process")`：
   `candidates_for_namespace("Std.IO")` = **空**（z42.io 尚未成为候选），
   Fallback-B 扫完那 8 个仍找不到 → 返回 `None`。
4. `interp::exec_object::obj_new` 在两处查找都落空时**静默合成一个空 `TypeDesc`**
   （`make_fallback_type_desc`：无字段、`id = UNRESOLVED`）→ 构造函数的 `FieldSet`
   因 `field_index` 查不到槽位而**被丢弃**，之后每次 `FieldGet` 都读到 `Null`。
   实测该对象 `refs=0 bytes=0 tid=4294967295`。
5. 崩溃点（`arr.Length`）离根因隔了两层调用，且**全程没有一条日志**。

**为什么变更前不炸**：boot 期 `force_load_all_declared()` 反复加载直到 `remaining_declared()`
为空 —— 顺带把候选集推到了**传递闭包**。它掩盖了 3 的窗口。

**同族的其它三处静默降级**（都被 boot 期 force-load 掩盖，实施中逐一撞到）：

| 静默点 | 表现 | 处置 |
|---|---|---|
| `resolve_type` 返回**基类尚未加载**的子类（`needs_fixup` 对此返 false）| 基类字段无槽位、`base(name)` 写入丢弃 → golden `vcall_base_fallback` 的 `Rex` 变 `null` | `ensure_base_chain_loaded`（迭代加载基类链 + fixup 到不动点）+ `base_chain_complete` 快路径 |
| `let _ = self.load_zpkg_file(...)` **吞掉加载失败** | 失败的包永远留在 `remaining_declared()` → 不动点循环原地死转 100% CPU | 「一轮无成功加载就停」的闸门 |
| `load_module_from_path` **丢弃**收集到的 `__static_init__` 名字 | z42b 显式加载编译器模块 → `DepScanCache._count` 永远 Null | 入队交给 `run_pending_static_inits` |

**外加一个顺序问题**：`static_fields_clear()` 把静态槽位清零后，已加载的包不会再被任何查找
重新触发 → 静态字段永远停在 Null（z42b 嵌套跑目标模块时 `Sha256._roundConstants` 就这么没的）。
处置：清零时把 `static_init_state` 里已跑过的名字**倒回待跑队列**，由紧随的排空重跑——
等价于变更前「清零后无条件重跑全部初始化器」的语义。

**修法（本变更）**：

- `resolve_function` / `resolve_type` 的 Fallback-B 改为**扫到不动点**：
  一轮加载完重新取 `remaining_declared()`，直到集合为空或命中。
  这在不恢复 boot 期全量加载的前提下，恢复了「可达即可解析」的保证。
- `obj_new` 的空描述符合成路径加 `tracing::warn!`（仅对含点号的跨包类名）。
  静默合成空描述符是数据损坏的温床——这个 bug 本可以第一时间被看见。
- `FieldGet` 的错误信息补上字段名（原来只说 "got Null"，不说读的是哪个字段）。

## 并发排空的收尾判据：不是「队列空」，而是「初始化静止」

新增的 cross-zpkg golden `static_init_concurrent`（两个工作线程同时首次触达同一包）
抓到一个真竞态，interp / JIT 两种模式都稳定复现：

1. 线程 A 与 B 同时进入 `resolve_function_tokens`（`Function::resolved` 是 `OnceLock`，
   只保证「一个赢」，不保证「后到者等前者跑完初始化器」）。
2. A 先排空：`mem::take` 把待跑队列整个取走，把 `Shared.__static_init__` 标成 `Running(A)`。
3. B 随后排空：**看到空队列**，直接返回 → 往下读 `Shared.Table` → **Null**（A 还在跑）。

第一版修法（把排空移到发布 `resolved` **之前**）不足以堵住——两个线程可以并发进入解析。

**最终判据**：`run_pending_static_inits` 的收尾改为 `await_init_quiescence()`——
等到 `static_init_state` 里**没有任何他线程的 `Running`** 才返回。

不会互相死等：本方法只等**别人**的 `Running`，而调用它时自己名下的初始化都已 `Done`
（`run_one_static_init` 返回即置 Done），构不成环。另设自旋上限兜底（持有线程真死了则放行 + 告警）。

修后 `static_init_concurrent`：**jit 40/40、interp 40/40**（修前两种模式都在 20–60% 概率炸）。
排空的收尾多了一次锁获取，实测无可见开销（hello 2.06×、z42i 6.49×，与修前一致）。

## 备选方案（已否决）

| 方案 | 否决理由 |
|------|---------|
| A：仅加 P4 守卫 | 无语义风险但收益小——hello 完全不受益（它的 18 个包来自 force-load 而非 Fallback-B）|
| B：zpkg META 增加 static-init 名单 | 走格式变更；#411 的两代自举段错误说明格式 bump 当前风险不可接受 |
| C：boot 时只跳过「无 `__static_init__`」的包 | 仍需读取每个包判断有无；实测 18 个包中 9 个无初始化器，只省 5.6/26 ms |
| D：在 `static_get_by_id` 加 miss 触发 | `Value::Null` 是合法值，无 miss 信号；且是热路径 |

## 实测收益（实现版 vs main 的 VM，同机 hyperfine）

| 场景 | main | 本变更 | |
|---|---|---|---|
| **`z42i -c '1 + 2'`（REPL 单次求值）** | 445.6 ms ± 14.0 | **69.5 ms ± 6.3** | **6.41×** |
| **`z42i` peak RSS** | 76.0 MB | **44.6 MB** | −41% |
| hello 墙钟 | 34.6 ms ± 5.9 | **17.6 ms ± 4.8** | 1.97× |
| hello peak RSS | 22.8 MB | **12.0 MB** | −47% |
| 用 Regex 的程序 墙钟 | 56.1 ms ± 7.7 | **21.2 ms ± 4.8** | 3.19× |
| z42c 编译 hello | 442.2 ms | 441.2 ms | 持平 |
| 加载 zpkg 数（hello） | 18 | **1** | |

REPL 是收益最大的场景——交互求值每次都要过完整的加载路径，变更前每轮都在为整个
标准库 + 编译器管线付 force-load 的钱。z42c 批量编译持平是预期内的（它本来就用大半标准库）。

## 原设计的预期收益（原型实测，同二进制切 env，hyperfine 60 runs）

| 场景 | 现状 | 目标 |
|---|---|---|
| hello 墙钟 | 32.1 ms ± 5.2 | **15.6 ms ± 5.0**（2.05×）|
| hello peak RSS | 22.7 MB | **11.4 MB**（−50%）|
| 用 Regex 的程序 墙钟 | 35.5 ms ± 4.6 | **19.1 ms ± 6.2**（2.27×）|
| 用 Regex 的程序 RSS | 22.7 MB | **11.8 MB** |
| z42c 编译 hello | 442.2 ms ± 8.0 | 441.2 ms ± 7.3（**持平**）|
| z42c peak RSS | 144.1 MB | 145.8 MB（持平）|

z42c 持平是预期内的：编译器本来就用到大半标准库，本来就要加载。
**本变更只惠及小程序 / 脚本 / CLI 工具 / REPL 启动，对编译器自身无收益。**

原型（不含 T3，故 golden 红）留档：`scratchpad/prototype-lazy-init.patch`。
