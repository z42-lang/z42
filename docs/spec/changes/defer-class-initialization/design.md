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

## 备选方案（已否决）

| 方案 | 否决理由 |
|------|---------|
| A：仅加 P4 守卫 | 无语义风险但收益小——hello 完全不受益（它的 18 个包来自 force-load 而非 Fallback-B）|
| B：zpkg META 增加 static-init 名单 | 走格式变更；#411 的两代自举段错误说明格式 bump 当前风险不可接受 |
| C：boot 时只跳过「无 `__static_init__`」的包 | 仍需读取每个包判断有无；实测 18 个包中 9 个无初始化器，只省 5.6/26 ms |
| D：在 `static_get_by_id` 加 miss 触发 | `Value::Null` 是合法值，无 miss 信号；且是热路径 |

## 预期收益（原型实测，同二进制切 env，hyperfine 60 runs）

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
