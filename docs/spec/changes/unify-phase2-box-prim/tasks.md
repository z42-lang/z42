# Tasks: unify Phase 2 —— R3 装箱统一（基元 → 堆 ScriptObject）

> 纯 runtime；spec-first（vm 变更）：DRAFT → **User 确认（D1/D2 拍板）** → IMPL → GREEN → PR。
> 环境：worktree z42-uvt，分支 `unify-phase2-box-prim`（基于 origin/main ed3fdcbe，格式 1.32/0.37，0.37 种子有效）。

- [ ] 阶段 0: DRAFT + **User 确认 D1（标量存 slots vs struct_bytes）/ D2（type_desc 来源）**
- [ ] 阶段 1: `__box_prim`（`corelib/convert.rs:9`）改产 `Value::BoxedStruct`（alloc ScriptObject，type_desc=wrapper，标量存 slots[0]）；保留 `Value::Boxed` 变体先跑通 → `cargo test --lib`
- [ ] 阶段 2: 逐个收敛 `Value::Boxed(b)` 双写 helper 臂（每改 `cargo test --lib`）：
  - [ ] 拆箱 `exec_object.rs:391`（AsCast/unbox，按 type_desc 分流：基元 wrapper→读 slots[0]，多字段 struct→拷 blob）
  - [ ] `convert.rs:212` value_to_str / `convert.rs:138` 拆箱取值
  - [ ] `reflection.rs:2013` GetValue / GetType（`object.rs:46`）/ SetValue
  - [ ] `types.rs:1184` GC visit / `arc_heap.rs:2005` 大小 / equality（`types.rs:1248` 区）
  - [ ] `repl.rs:159` 类名
- [ ] 阶段 3: 删 `Value::Boxed(Box<BoxedPrim>) = 13` 变体 + `BoxedPrim` struct；`grep -rn "Value::Boxed\b\|BoxedPrim" src/runtime` 清零（区分 BoxedStruct）
- [ ] 阶段 4: cargo `--lib` boxed-prim 单测 + golden e2e（引用身份/GetType/ToString/反射/roundtrip）
- [ ] 阶段 5: 全量 `xtask test`（含 cargo test）GREEN + self-host 不动点逐字节（编译器不动）+ 无格式 bump 核验 → 归档 + PR
