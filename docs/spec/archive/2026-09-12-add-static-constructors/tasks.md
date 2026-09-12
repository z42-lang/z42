# Tasks: 静态构造函数（C# 语义）

> 状态：🟢 已完成（2026-09-12）| 创建：2026-09-09
> GREEN：`xtask test` 全 stage ✅ / 完整 cargo 套件 ✅ / `test stdlib --mode jit` 331/331 ✅ /
> `test bootstrap` ✅ 无越界

## User 裁决
| # | 问题 | 裁决 |
|---|---|---|
| 语义 | 惰性 per-type vs 跟随 per-CU | **C# 语义**（惰性、按类型、首次使用前） |
| 屏障 | A 全量查 / B 仅有 cctor 的类型 / C 复用 per-CU | **B** |
| 异常 | v1 直接传播 / C# 包装 | **C# 语义**（类型标记失败 + 包装异常） |
| 触发点含静态方法调用 | 是/否 | **是** |
| 字段初始化器顺序 | A 拆出合成 per-type / B 豁免 / C 现状 | **A** |

## 完成项
- [x] `$Cctor` 类级哨兵（载荷 = cctor 发射函数名，零格式 bump）
- [x] `TypeEnv.InStaticCtor` → 放行本类 `static readonly` 赋值（兑现 #544 标注的耦合点）
- [x] 方案 A：有 cctor 的类，静态字段初始化器移入该类的类型初始化器（字段在前）
- [x] 静态 ctor 独立发射名 `$cctor`（修「实例构造器误调静态 ctor」）
- [x] per-type 状态机（`NotRun/Running/Done/Failed` + 同线程重入放行）
- [x] 无锁 `pending` 门（无 cctor 的程序屏障免费；全部跑完后重新免费）
- [x] 四个触发点 × 两个后端，**共用同一份实现**
- [x] `Std.TypeInitializationException` + 失败是终态、不重试
- [x] e2e 7 用例（interp + jit）+ Rust 单测 8 个
- [x] book：语言页 + 运行时机制页
- [x] GREEN 四道门

## 实现期发现的三个真 bug（都是「让特性真正跑起来」才暴露的）
1. **静态 ctor 与无参实例 ctor 撞名**（RegKey 都是 `C$0`）→ `new C()` 把静态 ctor 当实例
   构造器又跑一遍。此撞名一直存在，只因静态 ctor 从不执行而从未显形。
2. **`try_lookup_function` / `try_lookup_type` 只问惰性加载器** → 主合并模块里的函数/类型
   「明明存在却 not found」。
3. **JIT 侧完全没有屏障** → 用户代码默认走 JIT，默认模式下静态构造器根本不跑。

## 我自己设计里的两个漏洞（测试抓出来的）
1. `finish(Failed)` 也减 `pending` → 门短路 → `Failed` 分支永远检查不到 → 失败类型
   **静默变回可用**。改为只有成功才减。
2. 改 `methKey` 时连**体查找键**一起改了 → `model.HasBody` 落空 → 函数根本不发射。
   只能改发射名。

## 已知差距（已写入文档）
- **跨线程不等待**：他线程正在跑某类型 cctor 时本线程直接放行，可能看到部分初始化状态。
  C# 保证阻塞到完成。不做的原因是在持有解释器帧时阻塞极易与既有静态初始化排空逻辑
  （`DRAINING` / `init_batch_inflight`）互相死锁。
- 泛型类型的 per-实例化 cctor（C# 里 `C<int>` / `C<string>` 各跑一次）未做，v1 按开放类型一次。
