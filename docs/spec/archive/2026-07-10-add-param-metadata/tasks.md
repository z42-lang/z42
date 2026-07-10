# Tasks: 参数元数据（P1-d）

> 状态：🟢 已完成 | 完成：2026-07-10| 创建：2026-07-10 | initiative: unify-type-metadata P1-d

## 进度概览
- [x] 阶段 1: z42c（IrFunction MinArg/ParamsFrom/ParamNames + IrGen 三分支 + ZbcWriter/ZbcReader/ZpkgReader）
- [x] 阶段 2: Rust（Function.min_arg/params_from/param_names + read + 灌入）
- [x] 阶段 3: 反射（ParameterInfo IsOptional/IsParams/Name 权威）
- [x] 阶段 4: 版本 bump 25/29 + 两代自举 + regen fixtures/golden hex
- [x] 阶段 5: 测试 + 全 GREEN + 自举不动点
- [x] 阶段 6: 文档 + 归档

## 阶段 1: z42c
- [x] 1.1 IrModule: `IrFunction.MinArg:int`（默认 ParamCount）/ `ParamsFrom:int`（默认 0xFF）/ `ParamNames:string[]` + `ParamDefaultKinds:int[]`/`ParamDefaultI64:long[]`/`ParamDefaultF64:double[]`/`ParamDefaultStr:string[]`
- [x] 1.2 IrGen: **三发射分支**（normal ~166 / extern / abstract）填 MinArg/ParamsFrom/ParamNames + `_paramDefaults(md)` 折字面量默认值（IntLit/FloatLit/BoolLit/StringLit/CharLit/null→kind+值；非字面量→kind=0）
- [x] 1.3 ZbcFormat Minor 24→25
- [x] 1.4 ZbcWriter WriteSigEntries: method_flags 后写 min_arg(u16)+params_from(u8)；每参 param_type 后写 name_str_idx(u32)+default_kind(u8)+payload（含 this）+ 预扫 intern 参数名/str 默认值
- [x] 1.5 **读侧对称**：ZbcReader SIGS + ZpkgReader.ReadModuleSigs 消费 min_arg/params_from/name/default_kind+payload（非 gated 铁律）

## 阶段 2: Rust
- [x] 2.1 bytecode `Function.min_arg:u16` / `params_from:u8` / `param_names:Box<[String]>` + FuncSig 对应
- [x] 2.2 zbc_reader read_sigs 读 min_arg/params_from/每参 name；Function 灌入；bump 常量 25/29

## 阶段 3: 反射
- [x] 3.1 reflection.rs resolve_func_sig 返回 min_arg/params_from/names；build ParameterInfo 塞 IsOptional(pos>=min_arg)/IsParams(pos==params_from)/Name(SIGS 优先，DBUG 回退)
- [x] 3.2 ParameterInfo.z42 加 IsOptional + IsParams

## 阶段 4: bump + regen
- [x] 4.1 cargo build debug+release
- [x] 4.2 两代自举 0.28→0.29（gen1-stdlib EMPTY Z42_LIBS；快照 0.28 seed）
- [x] 4.3 regen zbc-format 6 + zpkg-format 4 + golden hex(empty/f5/selfcheck) + zpkg header pin + Rust pinned 25/29 + expected.json

## 阶段 5: 测试 + GREEN
- [x] 5.1 z42c golden 单测；Rust read 往返 + pinned
- [x] 5.2 reflection.z42 [Test]：IsOptional（必填+可选方法）/ IsParams（varargs 方法）/ Name（命名参数，权威）
- [x] 5.3 全 GREEN：xtask test ✅（e2e + cross-zpkg + stdlib 含 3 个 param_metadata [Test] + compiler/f5 + 不动点 7/7 + vscode）+ cargo lib 766+21

## 阶段 6: 文档 + 归档
- [x] 6.1 zbc.md/zpkg.md changelog + version-bumping 常量表 + reflection.md（参数元数据节 + ParameterInfo 成员表 + DefaultValue Deferred）+ roadmap Deferred Backlog Index
- [x] 6.2 归档 + ACTIVE.md 释放三锁 + commit/push + 盯 CI

## 备注
- **Scope 变更（User「一起做」）**：DefaultValue 的值编码**并入本砖**（原计划 P1-d2 延后）——SIGS 每参
  `default_kind:u8+payload`（0=无/1=null/2=i64/3=f64bits/4=bool/5=str idx），IrGen 折字面量；
  **非字面量**默认值（常量表达式/enum 成员）kind=0 → Deferred `fold-nonliteral-param-defaults`。
- 复用 P1-b/c 全套范式（两代自举 + 读侧对称 + fixtures regen）。**5 发射点全覆盖**（normal 方法/
  extern 桩/abstract 桩/impl 方法/自由函数——P1-c extern 漏设教训）。
- this 参数 name = "this"；min_arg/params_from/IsOptional/IsParams 用逻辑口径（不含 this），对齐 build_method_info 的 pos 计算。
- **Scope 外发现（不顺手修）**：`as string` 对 boxed str（object 槽持 Value::Str）返回 null——
  `interp/exec_object.rs as_cast` 只认 Object/Array/Null，boxed 原始值落 `_ => false`。pre-existing
  VM 语义缺口（`(bool)`/`(int)(long)` 走 Convert 路径正常，仅 as-cast 类分支缺原始类型臂）。
  测试改用 `dv.ToString()` 对账；缺口留独立 change（如 `fix-as-cast-boxed-primitives`）。
