# Tasks: struct 值语义 —— 内联/栈布局全重构（选项 B）

> 状态：🟡 进行中 — P1 阶段 6.5 gate 已过（2026-08-06）。选项 B + 3a + γ字节打包 + 分阶段 P1–P5。
> **每个 Pn 是独立 change 容器 + 独立 GREEN + 独立 PR + 独立归档**；本文件是 P1 的清单 + P2–P5 概览。
> 下一步：P1 阶段 1（StructLayout 字节布局）起实施。

## 程序概览（P1–P5，字节打包贯穿 —— User 裁决 γ=字节打包 + 分阶段）
- [ ] **P1** 字节布局地基 + 局部 struct 值语义（字节 blob 存储 + 复制 + 局部 lvalue）— 本清单展开
- [ ] **P2** 局部原地可变全场景补全（若 P1 收敛面未含）
- [ ] **P3** 对象字节内联 + `struct[]` 字节 backing + 元素原地可变（**converge [packed-primitive-arrays]**）
- [ ] **P4** 跨 zpkg 字节布局元数据（zpkg 格式 bump）+ 装箱/拆箱 + 反射一致
- [ ] **P5** JIT 值路径 + 密度/性能收尾（interp 全绿后）

## P1 进度概览
- [ ] 阶段 0: gate（Decision α/γ/格式时机/切分）+ 阶段 6.5
- [ ] 阶段 1: 编译器 StructLayout + 布局元数据
- [ ] 阶段 2: 寄存器区间分配 + struct-aware IR 指令
- [ ] 阶段 3: 运行时 struct 指令 dispatch + 区间复制/搬运
- [ ] 阶段 4: 局部 lvalue 原地可变（3a）+ 默认值 + 相等
- [ ] 阶段 5: 测试与验证（含自举不动点）
- [ ] 阶段 6: 文档同步 + 归档

## P1 阶段 0: gate（2026-08-06 "没问题" 通过）
- [x] 0.1 Decision α = base + 字节区间 `(base,byte_offset,size)` + 帧局部字节区
- [x] 0.2 Decision γ = 字节打包（v1 必做，converge packed-array）
- [x] 0.3 格式 bump 时机 = P1 只 bump zbc（新指令）；跨包 zpkg 布局推迟 P4；各走两阶段 nightly
- [x] 0.4 P1 收敛面 = 纯局部 struct（局部/参数/返回/嵌套局部 lvalue）；class struct 字段 + struct[] → P3
- [x] 0.5 阶段 6.5 gate 通过（User "没问题"）
- [x] 0.6 GC 读写屏障（User 强调）纳入 P1 硬约束（Decision ζ）——含引用叶子的局部 struct 即触发

## P1 阶段 1: 编译器 StructLayout（字节精确）
- [x] 1.1 `StructLayout.z42`（NEW）：每 struct 类型算 `{size, align, field_layout(byte_offset/size/kind)}`，
      嵌套递归展平 + 对齐排布 ✓（纯计算模块，输入 name→StructFieldsDef；6 单测全过）
- [x] 1.2 引用叶子位图/偏移表（带种类 ArcString/GcRef，供 GC 定位 + StructCopy 分流）✓
- [~] 1.3 自含值字段（无限大小）编译期报错：**环检测已做**（StructLayout.ErrorType，单测覆盖）；
      **E0416 诊断发射推迟到值语义生效（阶段 2/3）**——当前 struct 仍是引用语义，`struct Node{next:Node}`
      合法（Node 堆对象/next 引用），此刻报 E0416 会误杀现有合法程序
- [x] 1.4 struct-ness（**方案 B**：`Z42ClassType.IsStruct`，SymbolCollector 回填）+
      `StructLayout.BuildFromSymbols(SymbolTable)` 适配器（抽字段+预计算布局）✓；2 新单测；惰性
      （IrGen 实际调用随 阶段 2 codegen 消费接线）→ self-host 字节不动

## P1 阶段 2: 区间分配 + IR 指令
- [ ] 2.1 `FunctionEmitter.z42`：寄存器分配感知 width（struct 临时/局部/参数/返回占连续区间）
- [ ] 2.2 `IrInstr.z42`：新增 struct-aware 指令（StructCopy / StructFieldGet / StructFieldSet）——格式评估
- [ ] 2.3 `ExprEmitter.z42`：struct 表达式 → 区间；整体赋值/传参/返回 → StructCopy
- [ ] 2.4 `IrGen.z42`：区间复制 IR 生成
- [ ] 2.5 `IrEscapeAnalysis.z42`：struct 的 new 不走 stack_alloc（恒内联）

## P1 阶段 3: 运行时 dispatch（字节 blob）
- [ ] 3.1 `types.rs`：TypeDesc 承载字节布局 + 引用位图；Frame 增**局部字节区**存 struct blob
- [ ] 3.2 字节⇄Value 编解码（基元叶子 blob↔寄存器）
- [ ] 3.3 `exec_instr.rs`：新 struct 指令 dispatch（StructCopy/FieldGet/SetPrim…，无 `_` 兜底）
- [ ] 3.4 `mod.rs`：Frame blob 复制/搬运；collect_args / return 对 struct blob
- [ ] 3.5 `zbc_reader.rs` + `IrInstr` reader：新指令解码（bump zbc）

## P1 阶段 4: lvalue（3a）+ 默认值 + 相等
- [ ] 4.1 `ExprEmitter.z42`：局部 struct 嵌套字段 lvalue → 叶子字节地址直写（`line.a.x=3`）
- [ ] 4.2 TypeChecker：原地可变仅可寻址位置（rvalue struct 改字段报错）
- [ ] 4.3 默认值：struct 局部 blob 零初始化（各叶子默认，递归嵌套）
- [ ] 4.4 相等指令：struct blob 逐叶子值相等分支

## P1 阶段 4b: GC 读写屏障 + 根扫描（Decision ζ，User 强调）
- [ ] 4b.1 StructLayout 引用位图接入运行时类型元数据（GC 可查 blob 内引用叶子 byte offset）
- [ ] 4b.2 根扫描：帧局部字节区按引用位图扫描/更新 blob 内引用叶子（moving GC 可改写）
- [ ] 4b.3 `StructCopy` 分流：引用位图空 → 纯 memcpy；含引用叶子 → 值字节 memcpy + 引用叶子逐个写屏障
- [ ] 4b.4 `StructFieldSetPrim`（引用 kind 叶子）触发写屏障
- [ ] 4b.5 引用叶子读走统一访问点（预留读屏障接口，禁止绕过直读原始指针）
- [ ] 4b.6 对齐现有 `write_barrier_field` 分代/并发假设（热路径不漏屏障）

## P1 阶段 5: 测试与验证
- [ ] 5.1 `src/tests/struct-value-semantics/`：P1 场景 golden（[P1] 标记场景）
- [ ] 5.2 `codegen_tests.z42`：StructLayout + 区间复制 + lvalue IrDump 对比
- [ ] 5.3 Rust 单测：blob 复制语义 + GC 根扫描覆盖 struct 引用叶子 + 写屏障触发 + 无引用叶子走 memcpy 快路径
- [ ] 5.3b GC 压力 golden：含引用叶子的局部 struct 在 GC 后引用不被误回收（spec GC 场景）
- [ ] 5.4 `examples/struct_value_semantics.z42`
- [ ] 5.5 若 bump zbc：version-bumping.md checklist（writer/reader 常量、fixture regen、golden hex）
- [ ] 5.6 `cargo build --release`（z42vm）无错
- [ ] 5.7 `xtask test` 完整 GREEN（e2e / cross-zpkg / stdlib / compiler / vscode-syntax）
- [ ] 5.8 自举字节不动点（z42c/stdlib 源不用新语义 → gen1==gen2 不破；若 bump 格式走两阶段纪律）
- [ ] 5.9 若 bump 格式：`xtask test bootstrap` 边界检查（上一 nightly 能编当前源）
- [ ] 5.10 spec [P1] scenarios 逐条覆盖确认

## P1 阶段 6: 文档 + 归档
- [ ] 6.1 `docs/book/src/runtime/struct-value-semantics.md`（机制页：布局/区间/复制/lvalue/GC）+ SUMMARY
- [ ] 6.2 `docs/book/src/language/structs.md`（语言页值语义）
- [ ] 6.3 `escape-analysis-stack-alloc.md`：补注与值 struct 内联的区分（不同机制）
- [ ] 6.4 若 bump 格式：`docs/design/runtime/zbc.md`(+zpkg.md) changelog + version-bumping.md 常量表
- [ ] 6.5 目录 README（runtime interp / compiler semantics 入口变化）
- [ ] 6.6 `docs/features.md` + `docs/roadmap.md`（P1 完成、P2–P5 Deferred 索引）
- [ ] 6.7 `z42-structs-not-value-types` 记忆更新（进行中→部分修复）
- [ ] 6.8 归档 + PR（parallel-development §1.1 三段 body）

## P2–P5 概览（各自 change 容器补齐）
- [ ] P2: 局部 struct 嵌套 lvalue 已在 P1 阶段 4；P2 = 若 P1 收敛未含则补全局部原地可变全场景
- [ ] P3: 对象字节内联（δ）+ struct[] 字节 backing（β）+ 元素/字段原地可变 + IC name→byte_offset
      + **converge [packed-primitive-arrays]**
- [ ] P4: zpkg 字节布局元数据 + 跨包 struct e2e + boxing/unboxing（ε）+ 反射 + zpkg 格式 bump 全 checklist
- [ ] P5: JIT 值路径（struct 字节访问）+ bench/密度收尾

## 备注
- **P1 vs P2 合并**：3a 的局部 struct 嵌套 lvalue（`line.a.x`）本就在 P1 阶段 4，故 P2 主要覆盖
  P1 收敛面之外的局部原地可变；若 P1 gate 决定 P1 含全部局部可变，则 P2 直接并入 P1、程序变 P1/P3/P4/P5。
- **字节打包贯穿**：γ 裁决字节打包必做，故 P1 即引入字节布局地基（收敛 packed-array 字节机制），
  不再有独立"P5 字节打包"阶段；P5 变为 JIT + 性能收尾。
- **格式 bump 尽量集中**：Decision η——避免 P1（zbc 指令）与 P4（zpkg 布局）两次 bump 反复踩两阶段
  nightly 窗口；P1 gate 定策略。
- 本清单 P1 详、P2–P5 概览；每阶段开工前把该阶段展开为完整 tasks + Scope，走各自阶段 6.5 gate。
