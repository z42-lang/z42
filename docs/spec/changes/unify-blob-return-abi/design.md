# Design: blob struct 返回位的 ABI 统一

> 状态：🔴 DRAFT ｜ 前置：[proposal.md](proposal.md) ｜ 三个待裁决点：**D1 形状 / D2 刀数 / D3 ⑤ 的路线**

## 约束（先摆事实）

| # | 事实 | 证据 |
|---|---|---|
| C1 | sret 由**调用点的静态返回类型**决定 | `CallEmitter.z42:43,190`（`_isBlobStruct(c.Type())`）|
| C2 | sret 是**每方法固定**的 `method_flags bit3` | zbc 1.40；`IrModule.z42:325` |
| C3 | VM 按 `param_count + sret` 严格校验，失配即抛；**不许按 arity 自适应** | `symres.rs:195`；`symres_tests.rs:65`（`fix-call-arity-skew` 的门）|
| C4 | VCall **只按方法名索引 vtable 槽**，arity 不入解析键 | `CallEmitter.z42:168-171` |
| C5 | A′ 桥接范式已在 main：裸名放桥接、具体实现挪 `<m>$struct` | `IfaceBridgeSynth.z42`、`IrGenMemberEmitter.z42:43-46`、`EmitContext.z42:140,349-353` |
| C6 | 装箱 struct 现在**能读字段**（interp + JIT） | #838（`field_get` 的 `BoxedStruct` 臂）|
| C7 | 擦除返回位的静态类型就是裸型参 `T`（引用槽）⇒ 收到 `BoxedStruct` 与其余机制一致 | `--dump-bound` 实测 `(call id … :T)` |

C1×C2×C4 三条合起来就是矛盾的来源：**名字派发的入口不可能让每个调用点都知道要不要传 sret。**

## D1（待裁决）：怎么收口

### 形状 A′⁺ —— 把 #814 的桥接从「接口协变」推广成不变式【推荐】

> 凡**按名字/vtable 可达**的方法，其**裸名入口不要求 sret**：
> 返回 blob struct 者，具体实现挪 `<m>$struct`（带 sret），裸名放合成桥接
> （`struct_alloc` + `call <m>$struct` + `__box_struct` + `ret`，返回引用）。

- ✅ **零新机制**：C5 的三处落点已经在 main，本刀只扩**触发面**。
- ✅ 一次覆盖三副面孔（④a 泛型约束派发 / ⑤-a Money / ⑤-b 跨包 loose VCall）。
- ✅ 直接调用点（静态可解析）照旧绑 `$struct` ⇒ **无装箱快路径零开销、字节不变**。
- ✅ 与 #814 D1 的裁决一脉相承（「改在对的层」「零新 IR / 零格式 bump」「JIT 保持 ABI 静态」）。
- 🔴 **触发面判据要定**（见下「触发面三选一」），定宽了会给每个 blob 返回方法多合成一个函数。

### 形状 B —— VM 按目标 flags 自适应（❌ 不建议）

调用点带「允许 sret 适配」标记，VM 发现目标有 sret 而调用未给槽就自己分配。
**#814 D1 已经裁过同一个问题并否掉**：它把每方法固定的 ABI 变成**派发时协商**，
与「关键不变量靠构造式不变式」反向；且 JIT 要发条件化调用序列，否则接口/泛型调用整体降级回解释执行。
外加要动 wire 格式（zbc/zpkg minor bump + 两代自举）。**本刀不重新开这个口子。**

### 形状 C —— 只报错（止血）

「返回 blob struct 且可能被裸名派发」⇒ 编译错误，要求改返回类型。
- ✅ 最小、最安全。
- 🔴 直接把 `where T : INumber` 对多字段 struct 判死（`Vec2 + Vec2` 在泛型里永远不能用），
  也救不了 ⑤（`GCHandle.AllocStrong` 是 stdlib 现有 API，不能要求它改签名）。⇒ 只作退路。

### 触发面三选一（A′⁺ 之下）

| 选项 | 判据 | 代价 | 风险 |
|---|---|---|---|
| **T1 一律** | 所有返回 blob struct 的方法都建桥 | 每个这类方法 +1 合成函数（全仓返回 blob struct 的方法**实测 2 字段以上 struct 共 39 个声明**，方法数远小于它） | 最小认知负担、最难漏；桥接函数进 zpkg ⇒ 体积略增 |
| **T2 按可达性** | 接口实现 ∪ static-abstract 实现 ∪ **导出**（public/internal 跨包可见）方法 | 只给真可能被裸名派发的建桥 | 判据要在**生产方**算，且必须与消费方推导一致；漏判 = 运行期失配（今天这个 bug 的同款形状）|
| **T3 按调用点驱动** | 编译到某调用点时发现「按名字派发且目标返回 blob」再要求桥 | 最省 | 🔴 **不可行**：跨包时生产方已编完，消费方无法回头让它长出桥接 |

⇒ **建议 T1**。理由：T2 的判据必须「两侧都算得出且永不漂移」，而这正是本 bug 的根因形状
（`fix-call-arity-skew` 的门只在**失配时**响，判据漂移是静默的）；T1 让不变式**按构造成立**。
代价是可度量的（合成函数数量 = 返回 blob struct 的方法数），且桥接体只有 4 条指令。

## D2（待裁决）：要不要同刀放开「返回位代换」

`substitute-generic-call-return-type`（含 `TypeParamNames.Length` 那一行真 bug）在本刀之后才安全。
**建议分两刀**：

- 本刀是**约定收口**，判别力来自「三个已知失配用例从崩到跑通」，阴性对照干净。
- 返回位代换是**类型规则变更**，会让**全仓**凡泛型调用结果被使用处**发码变**（字节漂移、自举两代、
  fingerprint bump），并可能暴露一批新的诊断（`.X` 从不检查变成受检）。
  两件事混在一刀里，任一处红了都分不清是谁的。
- 代价：`id(v).X` 在本刀后仍走「装箱 + 按名取叶子」（#838 的路，~300ns/次），
  下一刀才提升为 `struct_fget_prim` 快路。**功能上不阻塞**。

## D3（待裁决）：⑤ 单字段 struct 走哪条路

**实测数据（2026-09-26）**：全仓 `.z42` 里 struct 按字段数分布 = `{0 字段: 36, 1: 10, 2: 33, 3: 4, 4: 2}`。
10 个单字段 struct 里**只有 2 个在发布库**：

| 类型 | 字段 | 特殊性 |
|---|---|---|
| `Std.GCHandle` | `private long _slot` | 有 `[Native]` **extern** 成员；`AllocStrong` 返回它 ⇒ ⑤-b 的触发者 |
| `Std.Guid` | `private byte[] _bytes` | 单字段是**引用**；抬头自记「`default(Guid)` 产 Null」等既知偏差 |

其余 8 个全是测试 fixture（含 `Money`，即 ④a 的触发者）。

### 路线 ⑤-blob —— 把 `IsBlobStruct` 的 `FieldCount < 2` 翻成 `< 1`

**已实测**（本 DRAFT 的取证实验）：编译器自举**构建成功**；e2e **355 passed / 2 failed**，
两条失败**全部**被本刀的 A′⁺ 覆盖（`Money.op_Add` 3/2、`GCHandle.AllocStrong` 2/1），**无第三种形态**。

- ✅ 与 2+ 字段 struct **同一个模型**（arena blob + `struct_fget_prim` + sret + `__box_struct`），
  无新表示、无新边界；机制全部已被 golden 覆盖。
- ✅ 一步拿到正确值语义（`S b = a; b.X = 50;` 不再改到 `a`）。
- 🔴 `GCHandle` 从裸 `long` 变 8 字节 arena blob ⇒ 每次 handle 操作多一次 arena 分配/拷贝；
  且它有 extern 成员 ⇒ **native 边界的返回表示要复核**（`__gc_handle_alloc` 返回 `GCHandle`）。
- 🔴 与归档设计的原话相反：`StructLayout.z42:305-307` 与 `ReprOf` 写着单标量叶子
  「保持现有模型（**标量塌缩 = Phase B**）」、`struct-value-semantics.md:926` 把
  「单标量叶子 struct 塌缩（`GCHandle`=Phase B）」列为 Deferred。

### 路线 ⑤-scalar —— 单标量叶子**塌缩**（归档设计的原意）

未装箱表示 = 那个叶子的裸值（`GCHandle` → `Value::I64`、`Guid` → 裸数组引用），
装箱时带**精确 TypeDesc**（`add-primitive-value-boxing` / unify Phase 2 R3 已有这条路，
`vcall_resolve` 的基元盒臂就是按精确类型派发的）。

- ✅ 值语义天然正确（复制标量 / 复制引用 —— 与 C# 对单字段 struct 的行为逐条一致）。
- ✅ `GCHandle` 保持裸 `long` ⇒ **native 边界零改动**、零 arena 开销（FFI 友好）。
- ✅ 契合归档原意，且复用「基元即值类型」那套已成熟的路。
- 🔴 **改动面大得多**：未装箱的裸值**没有类型标签** ⇒ `GetType` / `is` / `as` / 反射 / 重载决议
  在未装箱形态下都要靠编译期静态类型补齐（基元今天正是这么办的，但要为用户类型再走一遍）。
- 🔴 引入**第二个**值类型模型（Scalar 与 Blob 并存），`ReprOf` 的三分支要真正落地。

### 我的建议

**先做本刀（ABI 收口），⑤ 取 ⑤-blob**，把 ⑤-scalar 降级为**性能优化**另立（它真正的收益是
`GCHandle`/FFI 的零分配，而不是语义）。理由：

1. ⑤-blob 的爆炸半径**已实测**且**恰好等于本刀要修的那两条**；语义正确性一步到位。
2. ⑤-scalar 的语义收益与 ⑤-blob **相同**（都拿到值语义），但它要新建一个表示模型 ——
   在「先修 bug、不开新功能」的口径下，这属于后者。
3. 归档写「Phase B 塌缩」时，blob 那条路还没被 golden 铺满；今天它是全仓最成熟的值类型机制。
   ⇒ **设计原意值得尊重，但它的前提变了**（这条要写进 design 记录，不是默默推翻）。

⚠️ 若 User 取 ⑤-scalar：本刀（ABI 收口）**仍然必须做** —— ④a 在 main 上今天就崩
（双字段 `Vec2` 的 `where T : INumber`，与 ⑤ 无关）。只是 ⑤ 不再依赖它。

## 验收（判别力要求）

| 用例 | 修前 | 修后 |
|---|---|---|
| 泛型 `Add(Vec2, Vec2)`（双字段 + `where T : INumber`） | 🔴 `takes 3 physical, passes 2` | ✅ 得 `(11, 22)` |
| 既有 `src/tests/operators/static_abstract_operator.z42`（Money） | 绿（Money 非 blob ⇒ 2 对 2 巧合通过） | **仍绿**，且在 ⑤ 之后**才真的在测这条路** |
| `src/tests/gc/gc_handle.z42` | 绿（GCHandle 非 blob） | ⑤-blob 之后仍绿（本刀的桥接负责） |
| 直接调用点 `Vec2 r = Vec2.op_Add(a, b)` | 走 sret | **字节不变**（绑 `$struct`）|

- **阴性对照**：撤掉桥接合成 ⇒ 上表第一行必须回到 `takes 3 physical, passes 2`
  （不是改期望值，是撤机制）。
- ⚠️ **fingerprint / 缓存**：这类源文件**哈希不变而发码变**（多出桥接、具体实现改名）
  ⇒ 必须手动 bump `CompilerFingerprint`（理由照 #814 那档的写法）。无 wire 格式 bump。
