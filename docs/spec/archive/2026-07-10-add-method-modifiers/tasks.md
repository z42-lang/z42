# Tasks: 方法修饰符元数据（P1-c）

> 状态：🟢 已完成 | 创建：2026-07-10 | 完成：2026-07-10 | initiative: unify-type-metadata P1-c

## 进度概览
- [x] 阶段 1: z42c（IrFunction.MethodFlags + IrGen 填 + ZbcWriter/ZbcReader/ZpkgReader）
- [x] 阶段 2: Rust（FuncSig/Function.method_flags + read + 灌入）
- [x] 阶段 3: 反射（MethodInfo.IsVirtual 权威 + IsAbstract）
- [x] 阶段 4: 版本 bump 24/28 + 两代自举 + regen fixtures/golden hex
- [x] 阶段 5: 测试 + 全 GREEN + 自举不动点
- [x] 阶段 6: 文档 + 归档

## 阶段 1: z42c
- [x] 1.1 IrModule: `IrFunction.MethodFlags:int`（默认 0）
- [x] 1.2 IrGen: `_methodFlags(mods)`（virtual|override|abstract→bit0；abstract→bit1）；三处填（170/290/353 同 Visibility 址）
- [x] 1.3 ZbcFormat Minor 23→24
- [x] 1.4 ZbcWriter WriteSigEntries visibility 后写 method_flags(u8)
- [x] 1.5 **读侧对称**：ZbcReader SIGS + ZpkgReader.ReadModuleSigs 消费 method_flags（非 gated 铁律）
- [x] 1.6 **abstract 方法 emit（User 扩 scope）**：IrGen `_emitAbstractStub`（实例 abstract → signature-only 死体桩进 SIGS/FUNC）+ 方法发射 else 分支；限实例（`abstract ∧ ¬static`），INumber 静态抽象不动

## 阶段 2: Rust
- [x] 2.1 bytecode `FuncSig.method_flags` + `Function.method_flags` + METHOD_FLAG_VIRTUAL/ABSTRACT
- [x] 2.2 zbc_reader read_sigs visibility 后读 method_flags；Function 两处灌入；bump 常量 24/28

## 阶段 3: 反射
- [x] 3.1 reflection.rs resolve_func_sig 返回 method_flags；build_method_info 设 IsVirtual(flag∨vtable)/IsAbstract
- [x] 3.2 MethodInfo.z42 加 IsAbstract（IsVirtual 已存在）

## 阶段 4: bump + regen
- [x] 4.1 cargo build debug+release
- [x] 4.2 两代自举 0.27→0.28（gen1-stdlib EMPTY Z42_LIBS）
- [x] 4.3 regen zbc-format 6 + zpkg-format 4 + golden hex(empty/f5/selfcheck) + zpkg header pin + Rust pinned 24/28 + expected.json

## 阶段 5: 测试 + GREEN
- [x] 5.1 z42c golden 单测；Rust read 往返 + pinned
- [x] 5.2 reflection.z42 [Test]：test_method_modifiers（abstract class Shape：virtual/abstract/普通 → IsVirtual/IsAbstract）+ test_override_is_virtual_not_abstract（Square override）
- [x] 5.3 全 GREEN（xtask test）+ 自举不动点 + cargo

## 阶段 6: 文档 + 归档
- [x] 6.1 zbc.md/zpkg.md changelog + version-bumping 常量表 + reflection.md（方法修饰符反射节 + 成员表）
- [x] 6.2 归档 + ACTIVE.md 释放三锁 + commit/push + 盯 CI

## 备注
- 复用 P1-b 全套范式（两代自举 + 读侧对称 + fixtures regen）。
- **abstract-emit 踩坑（1.6）**：`MethodDecl.RetType` 是 `TypeExpr` 非 string，stub 须
  `ResolveType(md.RetType).Name()` 解析——否则 NamedType 对象污染 STRS 池崩溃。诊断靠逐条
  dump 池（z42 null 检查不可靠）。详见 design.md Decision 4b。
- Decision 2：IsVirtual = virtual∪override∪abstract（镜像 C#）；Decision 3：从 vtable 启发式收敛到声明 flag。
- **IsVirtual 权威化踩坑（3.1）**：z42 把**所有**方法（虚/非虚）都入 vtable，故 `flag ∨ vtable`
  会把非虚方法（如普通 `Sides()`）误报 virtual。修：`sig_found ? (flag&bit0) : vtable`——解析到
  SIGS 时 flag 权威、无 sig 才回退 vtable。
- **归档后回归修复（follow-up commit）**：`virtual extern` 方法（`Object.Equals/GetHashCode/
  ToString`）走 IrGen 的 `_emitNativeStub` 分支，此前**漏设 Visibility + MethodFlags** → IsVirtual
  权威化后误报 `false`（P1-c 引入的回归，因 P1-c 自身测试未覆盖 Object 方法故 CI 未拦到——User
  「是不是还没做完」一问逼出）。修：extern 分支补 `stub.Visibility=_visCode` +
  `stub.MethodFlags=_methodFlags`。纯 codegen、只动 stdlib(Object) 字节、z42c 无 virtual extern
  故不动点不受影响、无格式 bump。测试补 `test_method_modifiers` 加 ToString(继承 virtual extern)
  断言。GREEN 全绿 + 不动点 7/7。**教训**：P1-b 加 visibility 时也漏了这个 extern 分支（仅因都
  public 而 IsPublic 恰对）——**任何往方法元数据加字段的改动，三个发射分支（normal/extern/
  abstract）都要覆盖**。
