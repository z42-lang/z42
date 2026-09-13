# Tasks: 把「本类没有构造函数」缓存到 ObjNew 调用点（cache-ctorless-objnew）

> 状态：🟢 实施+验证完成，待合并 | 创建：2026-09-04 | 类型：perf（不改可观察语义 → 最小化模式）
> 前置：#421（cache-failed-name-resolution）。这是 #421 tasks.md 里列为 Out of scope 的那条，
> 现在有了 profile 数据把它排到队首。

## 背景（#421 之后的 profile）

同一个分配密集基准，#421 之后重新采样 1328 个（jit 模式）：

```
1328 exec_function_body
   ~640 jit_obj_new                       ← 仍占 48%
     289 alloc_object（真正的分配：finish_alloc / object_regions）
     234 resolve_id_by_name + try_lookup_function   ← 构造函数名解析，一次分配查两遍
      55 from_utf8（类名 + 构造函数名每次都验一遍 UTF-8）
      28 module.type_lookup(class_name)
```

#421 把**失败解析本身**从「全表扫描」降成「一把锁 + 两个哈希探测」，但没去掉
「每次分配都要问一遍」。234/1328 ≈ **18%** 还在。

## 实施

- [x] 1.1 `ResolvedTokens` 加 `ctorless_marks: Vec<AtomicUsize>`，与 `type_tokens`
      **同一套 ObjNew site 索引**（`resolve_function_tokens` 里并行构造）。
      语义：本 site 证明「`<Class>..ctor$N` 解析不到」时的注册计数；`0` = 从未证明。
- [x] 1.2 `resolver::ic` 加 `ctorless_hit` / `ctorless_note` 两个纯函数 +
      **进程全局** `FN_REGISTRATIONS` 计数器（`fn_registration_mark` / `note_fn_registration`）。
      **刻意做成全局**：它只被拿来做等值比较，跨 `VmContext` 共享只会让缓存显得更陈旧
      （→ 重新解析），不会让陈旧的答案显得新鲜。另外这样不动 `LazyLoader` / `VmCore`
      的布局（见 benchmarking.md 的「布局彩票」）。
- [x] 1.3 `LazyLoader::insert_function` —— **唯一**能让 `function_table` 变大的入口
      （registry.rs 两处插入循环都改走它），每次插入 bump 全局计数。
      loader 装/卸也 bump（换一个 loader 可能让原本不存在的构造函数出现）。
- [x] 2.1 interp `exec_object::obj_new` 与 JIT `jit_obj_new` 各接一个 site 槽指针；
      JIT 侧指针由 `ctorless_mark_ptr_at` 在 codegen 期烘进常量（与 `field_ic_ptr_at` 同款，
      槽位于 write-once 的 `Function.resolved`，地址对模块生命周期稳定）。
- [x] 2.2 **快路径仍先探合并模块**（`module.func_index`，一次哈希、无锁）：
      site 槽的语义是「**懒加载侧**没有这个构造函数」，而 `Function.resolved` 会被
      「这个函数可能运行于其中的每个 module」共用，所以带着构造函数的 module 必须照样赢。
- [x] 2.3 JIT 快路径放在构造 `ctor_args` **之前**，连那个 `Vec` 也省掉。
- [x] 3.1 前置 refactor（独立 commit，本变更把两个文件推过 500 行硬限）：
      `metadata/resolver.rs` 510 → 内联 `mod resolver_tests` 外移成 `resolver_tests.rs`（293）；
      `interp/exec_object.rs` 503 → 拆出 `exec_object_isa.rs`（411 / 97）。
      > `jit/helpers/object.rs` 本来也要拆（595 行），但 **#420 已在 main 上拆过**了
      > （拆成兄弟模块 `object_field.rs`，不是我原来的 `object/field.rs`）——rebase 时
      > 直接采用 main 的版本、丢掉自己那份，连带它的 `line-limit-baseline.txt` 更新也
      > 与 main 的完全一致，无需再动。本变更落在 main 的 `object.rs`（332 → 352 行）上。

## 验证

- [x] 4.1 `cargo test --lib` 1056 + 21 passed（含 3 条新的 ctorless IC 单测；用**合成 mark**
      而不是读全局，否则并行跑的其它测试 bump 计数会让断言 flaky）
- [x] 4.2 wasm32 检查 0 error
- [x] 4.3 `xtask test` ✅ GREEN
- [x] 4.4 A/B 实测

## 实测（同机 hyperfine；base = 本分支的 refactor commit 编出的 VM，行为等价 #421）

| 场景 | base(#421) | 本变更 | |
|---|---|---|---|
| `09_alloc_ctorless`（150 万 `new`）| 327.6 ms ± 9.0 | **268.6 ms ± 6.9** | **1.22×** |
| 1200 万 `new` | 2.554 s ± 0.024 | **2.094 s ± 0.026** | **1.22×** |

**累计**（本 session 起点 → 现在，同一负载）：**4.933 s → 2.094 s = 2.36×**。

2.2 的「快路径仍探合并模块」那一次哈希值 **1.5%**（不加是 1.24×，加了 1.22×）——
用 1.5% 换掉一整类跨 module 的静默错构造，划算。

hello 启动：instructions retired 69.6 M → 73.3 M。**又是布局彩票**，已按
`docs/book/src/dev/benchmarking.md` 的配方证伪：本变更**再加一个死字段**仍是 73.33 M
（不是 77 M）——效应会**饱和**而不是累加，说明是二选一的布局桶而不是真多干了活。

## 正确性论证（为什么等值比较是安全的）

一个原本解析不到的 `<Class>..ctor$N` 要变得可调用，只可能经由：
① 插进某个 loader 的 `function_table`（唯一入口 `insert_function`，每次 bump）；
② 换 loader（install / uninstall，都 bump）；
③ 出现在合并模块的 `func_index` 里（**快路径每次都探**，不靠缓存）。
三条全部被覆盖 ⇒ 「mark 没变」⇒「那个否定答案仍然成立」。

`ctorless_note` 存的是**解析之前**读到的 mark：若解析期间有注册插进来，存下的值当场就是
陈旧的，下次访问重新解析——保守方向，不会反过来。
