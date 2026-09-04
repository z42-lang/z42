# Proposal: 类初始化改为按需触发——把「启动加载整个标准库」拆掉

## Why

**一个 hello world 现在要加载 18 个 stdlib 包、2910 个函数，花 32.1 ms、占 22.7 MB RSS。**
其中 26 ms / 11 MB 是纯浪费：程序只用到 1 个函数。

根因是一条完整的因果链（每一环均已实测确认，2026-09-04）：

1. z42c 给 `hello.zbc` 的 IMPT 段写入 `["Std", "Std.IO", "Std.IO.Console"]` —— 含**无区分度的根命名空间 `Std`**。
2. 11 个 stdlib 包的 NSPC 段都声明了 `Std` → `app::build_declared_candidates` 得到 11 个候选包。
3. [`interp::init_static_fields`](../../../../src/runtime/src/interp/entry.rs) 为了**枚举** `*.__static_init__`，
   调用 `LazyLoader::force_load_all_declared()` —— 把全部候选包**整包加载并建索引**，再对函数名做后缀字符串匹配。
   候选包 + 其传递依赖 = 18 个包。
4. 独立缺陷：`LazyLoader::resolve_type("int")` 这类无点号的原生类型名没有 namespace 可路由，
   直接落入 Fallback-B「遍历所有未加载包，挨个加载再看看」，最终返回 `None`。

成本拆解（interp 模式实测）：

```
static-init: eager=16 fns, lazy collect(force-load all)=13.573ms, lazy run=31 fns 0.078ms
```

**为执行总耗时 78 µs 的 31 个初始化函数，花了 13.6 ms 去「找」它们。99.4% 的成本是发现，0.6% 才是执行。**

这段代码不能简单删除——它是**正确性机制**：静态字段初始化器必须在 `Main` 之前跑完。实测跳过后 z42c 立刻崩溃
（`type mismatch in comparison: I64(0) vs Null at Z42.Pipeline.DepScan.ScanDirs`）。

现状对标（同机 hyperfine）：z42 hello 32.1 ms vs `python3 -c print` 20.7 ms vs `node -e` 32.5 ms。
去掉浪费后 z42 应落在 15 ms 档，即 Python 之下。

## What Changes

把「启动时初始化所有已声明的包」改为 **CLR / JVM 式的按需类初始化**：一个包的 `__static_init__`
在该包**首次被真正用到**时执行，而不是在 `Main` 之前统一执行。

- **P1 触发点补全**：现有的懒加载只有函数查找 / 类型查找两条路径。新增**静态字段访问**触发点——
  `VmContext::static_get` / `static_get_by_id` 读到未初始化槽位时，触发所属包的初始化。
  这是当前架构缺失的一环：`static_get` 现在读到未知字段直接返回 `Value::Null`（[statics.rs:45](../../../../src/runtime/src/vm_context/statics.rs)），
  从不触发初始化——因为现状把一切都提前跑完了，从来不需要。
- **P2 待跑队列**：`LazyLoader::load_zpkg_file` 在注册函数时把 `*.__static_init__` 名字入队；
  `VmContext` 在**释放 loader 锁之后**排空队列执行（初始化器自身会再触发查找，必须放锁后执行）。
- **P3 根命名空间不参与候选路由**：`Std` 这类根命名空间匹配 11 个包，没有区分度。
  候选集只按最具体的命名空间路由。
- **P4 原生类型名守卫**：`int` / `long` / `string` 等关键字名不可能是 zpkg 导出的类名，
  直接返回 `None`，不走 Fallback-B 全量扫描。

## 语义变更（需 User 明确批准）

**现状**：所有*已声明*的包的静态初始化器都在 `Main` 之前执行，**包括程序根本用不到的包**。
**变更后**：只有被真正触达的包才执行其初始化器。

这与 CLR（`beforefieldinit` / 类型初始化器在首次静态访问时触发）和 JVM（`<clinit>` 在首次主动使用时触发）
一致，但对 z42 是**可观察的语义变化**：若某个初始化器有副作用（写文件、注册全局回调），
且其所在包未被程序引用，该副作用将不再发生。

已扫描：stdlib 的 31 个初始化器全部是纯表构造（S-box、关键字表、常量数组），无外部副作用。
用户代码的初始化器不受影响——用户包总是被引用才会进入候选集。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/lazy_loader.rs` | MODIFY | `pending_static_inits` 队列；`resolve_type` 原生类型名守卫（P2/P4）|
| `src/runtime/src/vm_context/lookup.rs` | MODIFY | `run_pending_static_inits()`；两处查找后排空（P2）|
| `src/runtime/src/vm_context/statics.rs` | MODIFY | 静态字段读未初始化槽 → 触发所属包初始化（P1）|
| `src/runtime/src/interp/entry.rs` | MODIFY | `init_static_fields` 不再 force-load；只跑主模块 + 已加载包 |
| `src/runtime/src/jit/mod.rs` | MODIFY | JIT 侧同一处理（现有 `collect_lazy_static_init_names` 消费点）|
| `src/runtime/src/app.rs` | MODIFY | 候选集不按根命名空间路由（P3）|
| `docs/book/src/runtime/` | MODIFY | 类初始化时机上浮为长期规范 |

## 不在 Scope

- zpkg / zbc 格式变更（本变更不动格式；`Std` 根命名空间是否继续写入 IMPT 属编译器侧，另议）
- 对象分配单块化、帧登记去锁等其它性能项
- REPL 自身的求值语义（仅需保证不回归，见 design.md「REPL 影响」）
