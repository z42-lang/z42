# Tasks: blob struct 返回位的 ABI 统一

> ⚠️ **本文件在 D1/D2/D3 裁决前只是草案**（形状未定 ⇒ 步骤会变）。取号、bump 等待批后再做。

## 0. 前置（已完成的取证）

- [x] ④a 在 main 上复现：双字段 `Vec2` + `where T : INumber` → `takes 3 physical, passes 2`
- [x] ⑤ 爆炸半径实测：`IsBlobStruct` 翻 `< 1` ⇒ 编译器自举**构建成功**、e2e **355/2**，
      两条失败全部是本刀的触发面（`Money.op_Add` 3/2、`GCHandle.AllocStrong` 2/1），无第三种
- [x] ⑤-b 性质钉死：真用例 `src/tests/gc/gc_handle.z42` 的调用点 IR 是
      `vcall %27.AllocStrong(%26)`（按名字派发、无 sret 槽），不是单文件探针的假象
- [x] 单字段 struct 普查：`{0:36, 1:10, 2:33, 3:4, 4:2}`，10 个单字段里仅 `GCHandle` / `Guid` 在发布库
- [x] 桥接范式可复用性：`IfaceBridgeSynth.z42` + `IrGenMemberEmitter` `emitKey` + `EmitContext` 剥名

## 1. 实施（形状 A′⁺ + 触发面 T1，待批）

- [ ] 1.1 触发面：把 `IfaceBridgeSynth` 的建桥条件从「接口声明返回引用型」扩到
      「**方法返回 blob struct**」（T1 一律）。⚠️ 沿 L2 教训：判据取自 **IrGen 的实际发射记录**，
      下游（`ClassDescBuilder`）读那张表，**不另拼第二份键推导**
- [ ] 1.2 static-abstract 运算符：确认 `op_X` 走同一条建桥路径（裸名给桥、实现挪 `$struct`）；
      `ExprTyper` 型参分支发的 `BoundCall("instance", …, 1 arg)` 一字不改（它本来就按裸名派发）
- [ ] 1.3 直接调用点：`MethodSymbol.CallKey()` 指 `$struct`（#814 已建此路，复核覆盖静态运算符路径
      `ExprTyper:682` 的 `opMs.RegKey`）
- [ ] 1.4 跨包：确认桥接进 zpkg 且**导入侧不重新合成**（#814 的同款要求）
- [ ] 1.5 `CompilerFingerprint` 手动 bump（理由：源文件哈希不变而发码变 —— 多出桥接 + 实现改名）

## 2. ⑤（D3 取 ⑤-blob 时）

- [ ] 2.1 `StructLayout.IsBlobStruct`：`FieldCount < 2` → `< 1`，并改写那段注释
      （原注释说单标量叶子「保持现有模型、塌缩=Phase B」⇒ 若改路线必须就地记明**为什么改**）
- [ ] 2.2 `ReprOf` 的三分支同步（单字段不再归 `""`）
- [ ] 2.3 `GCHandle` 的 native 边界复核：`[Native("__gc_handle_alloc")] extern GCHandle Alloc(...)`
      的返回表示（blob 化后 Rust 侧产出什么、是否需要 sret 约定）—— **这是 ⑤-blob 唯一的真风险点**
- [ ] 2.4 `Guid`（单字段是 `byte[]` 引用叶子）：确认 ref 叶子侧表路径正常，
      并复核它抬头自记的既知偏差（`default(Guid)` 产 Null）是否被本改动影响

## 3. 验收

- [ ] 3.1 新 golden：泛型 `where T : INumber` 对**双字段** struct（`Vec2`）—— ④a 的正面用例
- [ ] 3.2 既有 `static_abstract_operator.z42` / `gc_handle.z42` 保持绿
- [ ] 3.3 ⑤ 用例：`S b = a; b.X = 50;` ⇒ `a.X == 10`（单字段值语义）
- [ ] 3.4 **阴性对照**：撤掉桥接合成 ⇒ 3.1 必须回到 `takes 3 physical, passes 2`（撤机制，不改期望值）
- [ ] 3.5 直接调用点字节不变（`--emit-zbc` 对账；⚠️ golden 默认优化集会关掉若干 pass，
      需要时挂 `opt_all` sidecar）
- [ ] 3.6 `xtask test` 全仓 + `cargo test --lib`（debug、**不带名字过滤**）
- [ ] 3.7 `Z42_JIT_PROFILE=1` 确认热路径真进 JIT（若涉及 JIT 侧改动）

## 4. 文档

- [ ] 4.1 `docs/internals/src/runtime/struct-value-semantics.md`：新增「blob 返回位的 ABI」节；
      Deferred 列表里「单标量叶子塌缩（Phase B）」按 D3 结论改写（是降级为性能优化，还是仍是正解）
- [ ] 4.2 `docs/reference/src/language/structs.md`：单字段 struct 的值语义（若做 ⑤）
- [ ] 4.3 `docs/reference/src/language/generic-constraints.md`：`where T : INumber` 对多字段 struct 可用
- [ ] 4.4 `docs/roadmap.md`：本条 + `substitute-generic-call-return-type` 的依赖关系更新
