# Proposal: struct 裸字节内联进堆对象字段 + `struct[]` 字节 backing（P3b）

> 状态：🟡 DRAFT — struct 值语义程序第 3 阶段的 **P3b**（P3a 泛型容器装箱已合并 main #161）。
> User 已裁决两条关键设计分叉（见下 Decisions）；本 DRAFT 待 User 审批规范后进入 IMPL。

## Why

struct 值语义三部曲（`==`/装箱/对象协议 #154/#156/#158）+ P3a 泛型容器装箱（#161）合并后，struct 已能：
值复制/传参/返回、嵌套字段、`==` 值相等、装箱进 `object` 答全对象协议、进 `Dictionary<P,V>`/`List<P>`/`HashSet<P>`。

**唯一剩下的洞**：struct 存进**堆对象字段** `class C { Point pt; }` 与 **`Point[]` 数组**。今天字段类型是 struct 本身（非 `object`）时，赋值把**帧作用域的 `Value::StructRef{idx,frame_id}` 裸存进对象槽 → 帧退出即悬垂 use-after-free**（与 PR2a 修的 `object o=struct` 同类，但这条走不了装箱，因为静态类型就是 `Point`，不擦除到 `object`）。`Point[]` 同理：元素是帧作用域 handle。

P3b 补完这个洞，同时兑现 struct 值类型的**真正 payoff**：堆对象内**密度**（基元字节精确打包，逼近 C# 布局）+ **FFI 零 marshaling**（值类型字节布局可直喂 native）+ **零 per-field 堆分配**。这也是 [[packed-primitive-arrays]] 「② inline `struct[]`」一直等待的前置——本阶段与其**收敛**。

## What Changes

- **对象内联表示（运行时）**：`class C { Point pt; }` 的 `pt` **裸字节内联**进对象——对象携带一段字节区（`struct_bytes`），基元与 **16B 托管句柄引用叶子都内联进字节区**；对象类型描述符携带 **ref-offset 表**（`(off, kind)×n`，从 struct 布局导出）供 GC 扫描与写屏障定位引用叶子。**非**每字段一次堆装箱。
- **数组字节 backing**：`Point[]` 用 `len × struct_size` 字节扁平 backing（复用/扩展 `ArrayBacking` packed 机制），元素原地可变。
- **格式 bump（zbc 1.31→1.32 / zpkg 0.36→0.37）**：类描述符新增「哪些字段是内联 struct + 其在对象字节区的 byte offset + 对象字节总大小 + 内联 struct 的合成 ref-offset 表」。struct 自身的**带种类 ref 位图** A-use 已持久化（zbc1.31 TYPE section），R1 直接复用组合，无需重造。
- **GC（写屏障 + 扫描，本阶段核心难点）**：内联 struct 的引用叶子落在堆对象/数组的字节区内，不再是独立 GC 根重扫（arena 每采集重扫，故 P1/2a 无屏障）。需：① `scan_object_refs` / `trace_children` 按对象的 ref-offset 表**从字节区 unsafe 重建 `&Value` 访问**内联叶子；② 写内联引用叶子时按 offset 触发 `write_barrier_field`（并发/分代模式）。
- **编译器 codegen**：`class C{ P pt; }` 的字段 get/set 从「引用 FieldGet/FieldSet 存 StructRef」改为「按字段在对象字节区的 offset 发内联 struct 字节访问」；`arr[i].x` 叶子地址直写（3a 原地可变，收敛数组 backing）。

## Decisions（User 已裁决，2026-08-11）

- [x] **内联表示 = 裸字节内联（R1，最大密度）** — 引用叶子=真 16B 托管句柄内联进字节区；对象类型携带 ref-offset 表供 scan/barrier 定位。**非**侧表 `Box<[Value]>`（R2）、**非**只装箱（每字段堆 alloc）。取真 C# 布局 + FFI 零 marshaling payoff，代价=引入 unsafe 字节↔Value 重建 + 每对象类型带布局元数据 + 手工写屏障。
- [x] **一个 PR 全做** — class 字段内联（P3b-1）+ `struct[]` 字节 backing（P3b-2）同一 PR、同一次格式 bump 落地。

## ✅ DRAFT 审批已裁决（User，2026-08-11）

- [x] **Decision D1 = D1-a**（基元裸内联进 `struct_bytes` + 引用叶子走 `struct_refs: Box<[Value]>` 侧表）。事实校正后确认：密度/FFI payoff 全在基元打包（两路相同），引用叶子放侧表无密度损失且换回内存安全 + 与 arena/BoxedStruct 完全同构。**非** D1-b 全裸内联（unsafe/GC 物化临时/手工 Arc）。
- [x] **路线 α**（复用 StructFieldGetPrim/SetPrim 0xC0–0xC3，扩 `Value::StructRef` base 指向堆对象/数组内偏移，无新 opcode）。
- [x] **表示分裂 Open Question 消解**：D1-a 让堆内联与 arena/BoxedStruct 表示一致（都 bytes+refs 侧表），无分裂、无边界转码。

## Scope

见 `tasks.md` 的完整文件清单（编译器 codegen / 运行时对象·数组·GC / 格式 bump 六步 / golden / docs）。

## Out of Scope（Deferred，登记 roadmap）

- **JIT 值路径**（P5）：内联 struct 字段/数组元素访问的 JIT 支持；本阶段消费内联结果的 struct 指令使整函数 bail→interp（沿用 PR2a JIT 无 frame_id 策略）。
- **跨包内联布局 + 反射一致**（P4 剩余）：跨 zpkg 消费内联布局的完整反射；本阶段保证同包/已加载布局正确。
- **arena/boxed 统一成裸内联**（B-radical）：见上 Open Question 备选。
- **`readonly struct` / `ref struct` / `in` 零拷贝**：独立特性。

## 验证

interp 全绿门（`cargo test --lib` + `xtask test` **不传 Z42_HOME** + self-host 5/5 gen1==gen2）；格式 bump 走 version-bumping.md 6/9 步 + bootstrap-seed 两阶段 nightly（cold 路径靠 CI 两代自举）；GC 内联叶子的 scan/barrier 正确性用并发 GC 模式 golden + 悬垂校验。
