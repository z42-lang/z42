# Proposal: 跨包首次使用是「调静态方法」时，依赖包类型的静态构造器必须执行

> 变更类型：`vm`（完整流程）｜ 创建：2026-09-15 ｜ 来源：「推进 ctor 静默 bug」③（#643 发现）｜ 叠在 #650 之上

## Why

**依赖包里带静态构造器的类型，如果程序对它的第一次使用是调它的静态方法，它的静态构造器可能永远不执行——
且连带它的静态字段初始化器也不执行**（有静态 ctor 的类，初始化器注入在 cctor 体首）。不报错，读出类型默认值。

实测（当前 main + #650 构建，interp / JIT 结果相同）：

```z42
// 依赖包 demo.base
public class Cfg  { public static int level = 2; public static int Get() { return Cfg.level; }
                    static Cfg() { Cfg.level = Cfg.level + 40; } }
public class Loud { static Loud() { Console.WriteLine("Loud cctor"); } public static int Ping() { return 7; } }
```

| 主包 `Main` | 实际输出 | C# 期望 |
|---|---|---|
| `Cfg.Get(); Loud.Ping();` | `0` / `Loud cctor` / `7` | `42` / `Loud cctor` / `7` |
| `Loud.Ping(); Cfg.Get();` | `7` / `0` | `Loud cctor` / `7` / `42` |

结果**依赖调用顺序**：第一行里 `Loud` 的 cctor 之所以跑了，是因为前面解析 `Cfg.level` 时顺手登记了 `Cfg`，
把全局门打开了。

**根因**：cctor 屏障前有一道无锁门 `any_cctor_pending()`（「还有没跑完的已登记 cctor」）。登记点只有两处——
主模块合并后扫一遍（`app.rs`）、`try_lookup_type` 拿到跨包 TypeDesc 时。**跨包静态方法调用只查函数、不查类型**
⇒ 该类型从未登记 ⇒ 门读到 0 ⇒ `ensure_callee_owner_init` / `ensure_static_owner_init` 被整个短路。
屏障实现本身没问题，是「登记早于使用」这个前提在惰性加载路径上不成立。

**实施中发现的第二个成因**（只修登记点时实测：`Cfg` 已对，但首个 `Loud.Ping()` 仍跳过 cctor）：interp 与 JIT 的
**静态调用屏障都放在「解析被调函数」之前**。首次跨包调用恰恰是解析时才加载依赖包、登记其类型——屏障那一刻门仍是 0。

## What Changes

- 惰性加载器把类型并入注册表时，**同时登记该类型的 cctor**：两条加载路径（zpkg 文件 / 内存模块）的类型插入
  收敛到加载器内一个入口，入口里登记。与主模块「合并后全扫」同口径——**类型一进入可见范围就登记**。
- `try_lookup_type` 里的两处登记删除（被上面的入口完全覆盖，不留第二个登记点）。
- interp `exec_call::call` 与 JIT `jit_call` 的静态调用屏障**挪到被调函数解析之后**（interp 顺带把三条解析分支整理成
  「先得出目标 → 屏障 → 派发」，只保留一个屏障点）。
- 恢复 #643 为绕开本 bug 而写的 cross-zpkg 夹具（`static_property_cross_pkg` 等刻意不写静态 ctor 的注释与写法）。

## Scope（允许改动的文件）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/runtime/src/vm_context/cctor.rs` | MODIFY | 登记接口供加载器调用；注释里「登记点两处」改写 |
| `src/runtime/src/vm_context/types.rs` / `construct.rs` | MODIFY | `cctors` 改为可共享句柄（`Arc`） |
| `src/runtime/src/metadata/lazy_loader.rs` / `lazy_loader/registry.rs` | MODIFY | 持有登记句柄；类型插入单一入口 |
| `src/runtime/src/vm_context/lookup.rs` | MODIFY | 构造加载器时传句柄；删 `try_lookup_type` 两处登记 |
| `src/runtime/src/metadata/lazy_loader_tests.rs`（或同目录单测） | MODIFY | 加载即登记的单测 |
| `src/tests/cross-zpkg/static_ctor_crosspkg_static_call/` | NEW | 回归夹具（两种调用顺序） |
| `src/tests/cross-zpkg/static_property_cross_pkg/`、`static_property_crosspkg_readonly/` | MODIFY | 恢复静态 ctor 写法 |
| `src/tests/cross-zpkg/README.md` | MODIFY | 登记新夹具 |
| `docs/book/src/language/static-constructors.md` | MODIFY | 删「已知缺陷」段 |
| `docs/book/src/runtime/static-ctor-init.md` | MODIFY | 新增「登记点」一节（为什么在加载器入口登记） |

| `src/runtime/src/interp/exec_call.rs` / `jit/helpers/call.rs` | MODIFY | 静态调用屏障挪到解析之后（实施中发现，见 Why） |
| `src/tests/cross-zpkg/static_ctor_crosspkg_field_first/` | NEW | 守卫：首次使用是读静态字段 |

**只读引用**：`src/runtime/src/app.rs`（急切登记点，不改）。

## Out of Scope

- 跨线程等待（`static-ctor-init.md`「已知差距」）——另一个问题。
- 急切合并路径的登记（`app.rs:295`，#649 正把它挪进 `boot.rs`）——不动，避免与 #649 撞车。

## Open Questions

- 无（方案与代价见 design.md，需 User 确认）。
