# Spec: 逃逸分析驱动的栈上分配

## ADDED Requirements

### Requirement: 逃逸分析 pass 标记不逃逸分配

`IrEscapeAnalysis` pass 在 `Opt.StackAlloc` 开启时运行，对每个函数用流不敏感 may-escape 过近似判定
`ObjNew` / `ArrayNew` / `ArrayNewLit` 结果是否逃逸本函数帧，不逃逸者置 `StackAlloc=true`。

#### Scenario: 纯局部数组不逃逸

- **WHEN** 函数内 `new int[3]` 的结果 reg 只被 `ArraySet`(index/array 位)、`ArrayGet`、`ArrayLen` 读，
  从不被返回 / 传参 / 存入其它对象字段或数组元素 / throw / 闭包捕获
- **THEN** 该 `ArrayNewInstr.StackAlloc == true`

#### Scenario: 纯局部对象 + this-safe ctor 不逃逸

- **WHEN** `new Point(a,b)` 结果 reg 只被 `FieldGet`/`FieldSet`(receiver 位) 读，从不流出，**且** `Point`
  的 ctor body 只对 `this` 做字段初始化、不把 `this` 存静态 / 传给别的调用 / 返回
- **THEN** 该 `ObjNewInstr.StackAlloc == true`

#### Scenario: 返回即逃逸

- **WHEN** 分配结果 reg（或经 copy 传递到的 reg）出现在 `RetTerm` 的返回值位
- **THEN** `StackAlloc == false`（保持堆分配）

#### Scenario: 存入字段 / 数组元素即逃逸

- **WHEN** 分配结果 reg 出现在 `FieldSet` / `ArraySet` / `StaticSet` 的**值**操作数位
- **THEN** `StackAlloc == false`

#### Scenario: 传给任何调用即逃逸（含方法 receiver）

- **WHEN** 分配结果 reg 出现在 `CallInstr` / `VCallInstr` / `StaticCallInstr` 的任一实参或 receiver 位
- **THEN** `StackAlloc == false`

#### Scenario: 被闭包捕获 / throw 即逃逸

- **WHEN** 分配结果 reg 出现在 `MkClosInstr` 捕获位或 `ThrowTerm` 异常位
- **THEN** `StackAlloc == false`

#### Scenario: ctor 泄漏 this → 对象逃逸

- **WHEN** `new Foo()` 结果本函数内不逃逸，但 `Foo` 的 ctor 把 `this` 存入静态字段（或传给另一函数、返回）
- **THEN** 该 `ObjNewInstr.StackAlloc == false`

#### Scenario: 未知指令读该 reg → 保守判逃逸

- **WHEN** 分配结果 reg 被规则表未登记角色的指令读取
- **THEN** `StackAlloc == false`（安全兜底，绝不误判不逃逸）

#### Scenario: 多定义命名局部 → 保守判逃逸

- **WHEN** 分配结果被 copy 进一个有多个定义点的命名局部寄存器
- **THEN** `StackAlloc == false`（v1 只标单赋值 temp）

#### Scenario: pass 关闭时零改动

- **WHEN** `optSet` 不含 `Opt.StackAlloc`（如 debug -O0）
- **THEN** pass 不运行，所有分配指令 `StackAlloc` 保持默认 `false`，产物与不含本 pass 的 IR 逐字节一致

### Requirement: OptSet 独立开关

`Opt.StackAlloc` 作为独立位加入 OptSet，可与其它 pass 任意组合，单独开启必须正确（D2 独立性硬约束）。

#### Scenario: 位与名称解析

- **WHEN** 查询 `Opt.ByName("stack-alloc")`
- **THEN** 返回 `Opt.StackAlloc`（=64）；`Opt.All` 含该位（=127）

#### Scenario: profile 默认

- **WHEN** release 构建无 CLI/toml 覆盖
- **THEN** `Opt.StackAlloc` 启用；debug（-O0）不启用

#### Scenario: CLI / toml 覆盖

- **WHEN** `--no-opt stack-alloc`（release）或 `[optimize] stack-alloc=false`
- **THEN** 该 pass 关闭，其余 pass 不受影响

### Requirement: 运行时栈上分配语义（interp）

interp 遇 `StackAlloc=true` 的分配指令时在帧局部 arena 分配，产出 `Value::StackObject`/`Value::StackArray`，
帧退出即释放，不进堆、不受 GC 追踪；其字段/元素读写语义与堆对象/数组完全一致。

#### Scenario: 栈对象字段读写等价堆对象

- **WHEN** 对一个 `StackObject` 执行 `FieldGet`/`FieldSet`
- **THEN** 行为与等价 `Object` 逐位相同（读到写入值、初始值为字段类型默认）

#### Scenario: 栈对象持堆字段引用不被误回收

- **WHEN** 栈对象某字段存有堆对象（如 `String`）的引用，其间发生 GC
- **THEN** 该堆对象被根扫描器经 arena slot 标记存活、不被 sweep；GC 后字段仍可用

#### Scenario: 帧退出释放

- **WHEN** 创建了栈对象/数组的函数返回
- **THEN** 帧 arena 随帧 drop 释放，无 GC 参与，无内存泄漏

#### Scenario: JIT 语义等价（interp-first）

- **WHEN** 同一含 `StackAlloc=true` 分配的函数分别由 interp 与 JIT 执行
- **THEN** 两者输出逐字节一致（JIT 忽略 flag 照常堆分配，interp 栈分配；表示不同、可观察语义相同）

## MODIFIED Requirements

### Requirement: zbc/zpkg 分配指令 wire 格式

**Before:** `ObjNew`/`ArrayNew`/`ArrayNewLit` 的 zbc 编码尾部无栈分配标志。zbc `1.28` / zpkg `0.33`。
**After:** 三指令编码尾部各加一个 `u8` 栈分配标志（`1`=栈 / `0`=堆）。zbc bump `1.29` / zpkg bump `0.34`；
reader strict-pin 精确匹配新版本，旧产物不可读（pre-1.0 不留兼容，regen 重生）。

## IR Mapping

| 语法/场景 | IR 指令 | 新字段 | zbc 编码 |
|---|---|---|---|
| `new T(args)` | `ObjNewInstr` | `bool StackAlloc` | 原编码 + 尾 `u8` |
| `new T[n]` | `ArrayNewInstr` | `bool StackAlloc` | 原编码 + 尾 `u8` |
| `[a,b,c]` | `ArrayNewLitInstr` | `bool StackAlloc` | 原编码 + 尾 `u8` |

## Pipeline Steps

- [ ] Lexer —（不涉及）
- [ ] Parser / AST —（不涉及）
- [ ] TypeChecker —（不涉及）
- [x] IR Codegen — 新 `IrEscapeAnalysis` pass（`IrOptPipeline` 门控）+ 三指令加字段 + zbc 编码
- [x] VM interp — 帧 arena + `Value::StackObject/StackArray` + 字段/元素读写 + GC 根扫描
- [x] VM JIT — 读取新 zbc 字段但忽略（照常堆分配，语义等价）
