# 静态构造函数的按类型初始化

> 语义面见[静态构造函数](../language/static-constructors.md)。本页讲实现。
> 代码：`src/runtime/src/vm_context/cctor.rs`。

## 为什么不能照抄 C#

C# 的静态构造器屏障之所以「零成本」，是 **JIT 在类型初始化完成后把检查从机器码里 patch 掉**。

z42 的 JIT **没有代码 patch / 失效机制**——`FnEntry` 一旦 `OnceLock::set` 就永久有效，
无 deopt、无重编译。所以屏障只能是「每次访问都检查」，问题就变成**怎么让这个检查便宜**。

## 怎么让屏障几乎免费

用一个无锁计数 `cctor_pending`（「还没跑完的静态构造器类型数」）当门：

```rust
if any_pending() { ensure_type_init(...) }   // 热路径的全部代价
```

- 程序里**没有**静态构造器 → 计数恒 0 → 屏障就是一次 relaxed load
- 有静态构造器 → 只在它们**跑完之前**付查表代价；**全部跑完后计数归零、屏障重新免费**

「无锁镜像只用于 `== 0`（可证无事可做）方向」不是新发明——旁边的
`pending_type_init_count` / `running_static_inits` 用的是同一套手法。

> ⚠️ **失败时计数不减。** 失败的类型必须让门继续开着，否则屏障短路、`Failed` 分支
> 永远检查不到，失败类型会**静默变回可用**。代价是一旦有类型初始化失败，门就长期开着
> ——那已是致命错误路径，正确性优先。

## 状态机

```mermaid
stateDiagram-v2
    [*] --> NotRun: 加载期发现 $Cctor 哨兵
    NotRun --> Running: claim 成功（记下线程 id）
    Running --> Done: cctor 正常返回（pending--）
    Running --> Failed: cctor 抛出（pending 不减）
    Running --> Running: 同线程重入 → 直接放行
    Done --> Done: 后续访问无操作
    Failed --> Failed: 后续访问一律抛 TypeInitializationException
```

## 触发点与共用实现

| 触发 | interp | JIT |
|---|---|---|
| 静态字段读 / 写 | `exec_object::static_get/set` | `jit_static_get/set` |
| 创建实例 | `exec_object::obj_new` | `jit_obj_new` |
| 静态方法调用 | `exec_call::call` | `jit_call` |

**两个后端调的是同一份实现**（`ensure_static_owner_init` / `ensure_callee_owner_init` /
`ensure_type_init`）。这不是洁癖：两后端语义一致正是本特性最容易出错的地方，各写一份迟早
漂移。开发期就吃过一次——JIT 侧一度完全没有屏障，而**用户代码默认走 JIT**（只有
`__static_init__` 被强制走解释器），于是默认模式下静态构造器根本不跑。

`ObjNew` 手上已有 `TypeDesc`，所以那里**不需要门**——`td.cctor_func()` 一次
`Option` 判断就结束。

## 编译期怎么把信息传过来

类带静态构造器时，编译器在**类级 attr-ref 块**挂 `$Cctor` 哨兵，载荷是 cctor 的**发射
函数名**（FQ）。零格式 bump（先例 `$Deprecated`）——`class_flags` 是 u8 且已用满 8 位，
没有空位可用。

载荷带函数名而不只是个布尔，是为了让运行期**不必按约定拼名**：编译期命名与运行期拼名
两处规则一旦分开写就会漂移。

`build_type_registry` 是**所有**模块（急切 + 跨包惰性）的统一漏斗，在那里读一次哨兵、
把函数名放进 `TypeDesc` 冷区，就不必给每个 type-registry 插入点各挂一遍钩子。

> ⚠️ **静态 ctor 的发射名必须与实例 ctor 区分开。** 静态 ctor 的 `RegKey` 与**无参实例
> ctor** 逐字相同（都是 `C$0`），`new C()` 查 0 参构造器会命中静态 ctor 并把它当实例
> 构造器再跑一遍。发射名因此改用 `$cctor`（`$` 非法于标识符 → 零撞名）。
> 但**只能改发射名**——`methKey` 还兼作 SemanticModel 的体查找键（body 按 RegKey 存），
> 两者一起改会让函数根本不发射。

## 字段初始化器去了哪

**有**静态构造器的类，其静态字段初始化器由编译器从按编译单元的 `__static_init__` 移出、
注入该类静态 ctor 的**体首**——这样「字段初始化器在前、cctor 体在后」的 C# 顺序才成立。

两者若分处两个函数，cctor 被触发时 `__static_init__` 可能还没跑完，会出现
「cctor 写了 42、随后 `__static_init__` 又覆写回 1」的错值。

**没有**静态构造器的类保持原样（继续走按编译单元的急切初始化）——这本来就合规，
且让屏障的代价只落在真正用了静态构造器的类型上。

## 已知差距

跨线程等待未实现：他线程正在跑某类型的 cctor 时，本线程直接放行而非阻塞，可能看到部分
初始化状态。C# 保证阻塞到完成。不做的原因是在持有解释器帧时阻塞极易与既有静态初始化排空
逻辑（`DRAINING` / `init_batch_inflight` 那套）互相死锁。
