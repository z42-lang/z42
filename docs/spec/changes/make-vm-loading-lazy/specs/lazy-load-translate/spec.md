# Spec: VM 全惰性加载

## ADDED Requirements

### Requirement: 统一惰性加载路径（阶段 1，无格式改动）

#### Scenario: jit 不再 eager 合并整套闭包
- **WHEN** `--mode jit` 运行一个只调用 z42.core 的程序
- **THEN** VM 不 eager BFS 合并整个 dep 闭包;只在首次调用某 stdlib 函数时经 `try_lookup_function`
  加载其 zpkg(与 interp 同一收口)

#### Scenario: 惰性加载的 stdlib 函数以 JIT 原生码执行
- **WHEN** jit 下首次调用一个 lazy-loader 提供的 stdlib 函数 F(可翻译)
- **THEN** F 被 `compile_one` 编译为原生码执行(**非** `cross_zpkg_via_interp` 解释回退)——删 eager
  合并不得让 stdlib 退化到 interp

#### Scenario: 惰性 static-init 正确执行
- **WHEN** 入口依赖的 stdlib 命名空间含 `.__static_init__`
- **THEN** 该 static-init 被发现并执行(不因"只扫 merged module.functions"而漏)

### Requirement: 函数按需 parse（阶段 2）

#### Scenario: 只 parse 被调用的函数
- **WHEN** 一个程序调用某 zpkg 中 K 个函数(该 zpkg 共 N ≫ K 个)
- **THEN** 只有这 K 个函数的字节码被 parse 成 `Function`(经 FIDX 定位);其余 N−K 个永不 parse

#### Scenario: 首次调用后缓存
- **WHEN** 同一函数被调用两次
- **THEN** 第一次 parse + 填 `OnceLock` 槽,第二次直接命中缓存,不重新 parse

### Requirement: 类型按需物化（阶段 2）

#### Scenario: 只物化被使用的类型
- **WHEN** 程序使用某 zpkg 中 J 个类型(该 zpkg 共 M ≫ J 个)
- **THEN** 只有这 J 个类型建 `TypeDesc`/vtable(经 TIDX);其余永不物化

#### Scenario: 跨 zpkg 继承 base-first 物化
- **WHEN** 物化类型 T,其基类 B 在另一 zpkg 且未物化
- **THEN** 先经 `try_lookup_type` 物化 B(递归 base 链),再算 T 的 flattened 布局/vtable——结果与
  eager 全量 `build_type_registry` 逐字节一致

### Requirement: 格式 FIDX / TIDX 随机访问（阶段 2）

#### Scenario: 单函数/单类型随机寻址
- **WHEN** 读一个含 FIDX/TIDX 的 zpkg
- **THEN** 可按 `name → offset` 取单个函数/类型字节,无需 parse 整个 FUNC/TYPE 段

#### Scenario: 无索引旧格式回落（bump 分阶段窗口）
- **WHEN** reader 读到无 FIDX/TIDX 的（上一版 writer 产的）zpkg
- **THEN** 回落到全 parse 路径,行为正确(support 先行纪律,见 design D5)

## MODIFIED Requirements

### Requirement: JIT/AOT 启动依赖加载

**Before:** `main.rs` `is_eager` 对 jit/aot 做 transitive BFS 合并,把整个 dep 闭包 `merge_modules`
成一个 `final_module`,所有 dep 函数进 `func_index`。

**After:** 删 eager 合并(jit 走 lazy loader);dep 函数经 `try_lookup_function` 按需加载 + JIT 编译。
(aot 若需 eager 全编,单独在 aot 落地时处理,不在本 change。)

### Requirement: `Module` 函数/类型存储（阶段 2）

**Before:** `Module.functions: Vec<Function>`(全 parse)、`type_registry`(全 build)。

**After:** 存原始 FUNC/TYPE section 字节 + FIDX/TIDX 索引 + `OnceLock<Function>` / `OnceLock<TypeDesc>`
惰性槽;首次使用才 parse/物化填槽。

## Pipeline Steps

- [x] IR Codegen（ZbcWriter）— 阶段 2:emit FIDX/TIDX
- [x] VM 加载器（loader/zbc_reader/lazy_loader）— 阶段 1 统一路径 + 阶段 2 按需 parse
- [x] VM interp — 收口不变（`try_lookup_function/type`），受益于按需
- [x] VM JIT — 阶段 1:`resolve_fn_by_id` 统一编译 lazy-loaded 函数

## IR Mapping

阶段 2:zbc·zpkg 新增 `FIDX`/`TIDX` section,minor bump(strict-pin 两端同步)。无新 IR 指令/无
opcode 语义变更——纯加载/物化时机 + 格式索引。
