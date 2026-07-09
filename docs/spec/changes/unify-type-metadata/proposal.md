# Proposal: 统一类型元数据（删 TSIG，反射即真相源）

## Why

zpkg 现在把类型信息存**两份**:

- **TYPE / SIGS**（在 MODS 内，运行时段）:VM 执行 + 反射读的类型描述（字段/vtable/接口/attribute/…）。
- **TSIG / IMPL / EXPT**（独立段，编译期专用）:z42c 跨包解析读的"导出类型接口"。

两份由同一趟 IR 同源 emit（不漂移），但 **TSIG 把整个类型结构又存一遍**——z42.core 里 TSIG
占 48%、编译期三段合计 49%。它之所以存在,只因为运行时那份 **TYPE/SIGS 还不完整**:缺可见性、
virtual/abstract、默认参数、enum 成员值、delegate、跨包 impl 关联。

关键洞察(2026-07-09 与 User 定案):**这几样"缺的"恰恰是 roadmap C 流「反射完整化」要暴露的**
（`IsEnum` + `Enum.GetValues`、接口成员枚举、`IsPublic`/`IsVirtual`、默认参数、`GetInterfaces`
含跨包 impl）。即 **TSIG 不是"编译期专用",而是"反射还没做到的部分"的临时副本**。一旦按 C 流把
TYPE/SIGS 补成反射完整,TSIG 就是 100% 冗余。

**「补全反射」与「删掉 TSIG 重复」是同一件事,做一次。** 终态:一份反射级类型元数据
（TYPE/SIGS + 极小 impls 表），**同时**服务编译期解析、运行时反射、执行——即 C#/.NET 的
「单一元数据」模型（吸收其单一真相源 + 富 flag + Constant 表；避开其 ECMA-335 表爆炸 + RID token，
保留 z42 扁平命名段 + FQ 名的简单模型）。

不做:持续携带 ~24% 的编译期冗余（每个 zpkg,SDK 与部署皆然）,且反射永远差一截（拿不到 enum
值/可见性/跨包 impl 接口）。

## What Changes（三阶段 initiative，跨多个 change / 多个 nightly）

- **P1 超集**（= roadmap C 流反射补全）:把缺的字段 additive 加进 TYPE/SIGS/enum/delegate/impls,
  VM 反射暴露之。TSIG 照旧不动。→ TYPE/SIGS 成为 TSIG 超集 + 反射补全落地。
- **P2 对账**:z42c 跨包解析改为**从 TYPE/SIGS/impls 重建导出接口**,与 TSIG **双读对账**
  （断言逐字段一致）。不删 TSIG。→ 证明重建正确、零行为变化。
- **P3 删**:对账干净后,删 TSIG + EXPT 的 emit 与 z42c 读取（EXPT 由可见性派生);IMPL 段
  **保留但重新定性**为统一元数据(VM 也读,支撑跨包 impl 反射)。→ 单一真相源,每个 zpkg 变小。

> 每阶段 support 先行、晚一个 nightly 再 use（自举纪律 bootstrap-seed.md）,分多个 zbc/zpkg
> minor bump。每个可实施 change 单独走 workflow（proposal/design/spec/tasks + 6.5）。

## Scope

本文件是 **initiative 级 proposal + design**（目标架构 + 逐字段映射 + impl 设计 + 三阶段）。
**不含实施代码 Scope**——各阶段的具体 change 各自建容器、各自声明文件 Scope。首个可实施 change:
**P1 第一砖 = enum 值进 TYPE + `IsEnum`/`Enum.GetValues` 反射**（roadmap 0.3.12 已排）。

**只读引用**（设计依据）:

- `src/runtime/src/metadata/bytecode.rs`（ClassDesc/FieldDesc）、`zbc_reader.rs`（FuncSig/read_type）
- `src/compiler/z42c.project/src/ZpkgWriter.z42`（_buildTsig/_buildImpl 现结构）
- `src/compiler/z42c.semantics/src/{ExportedTypeExtractor,ImportedSymbolLoader}.z42`（_extractImpls/_mergeImpl）
- `docs/roadmap.md` C 流反射完整化
- `docs/design/runtime/{zbc,zpkg}.md`

## Out of Scope

- **Reference-assembly（剥方法体、留元数据）**:C# 的另一维度优化（编译-only 分发去掉 MODS func
  体）。与本 initiative 正交,记 Deferred,不在此设计。
- 泛型完整反射（MakeGenericType / 泛型 Method.Invoke）:roadmap 0.5.x,本 initiative 只保证
  元数据承载,不实现泛型运行时。

## Open Questions

- [ ] delegate 在 TYPE 里的表示:作"带 Invoke 方法的特殊类"（C# 式）还是独立轻量记录?（design D5 倾向前者）
- [ ] P1 各字段的 additive bump 是合成一个大 bump 还是逐字段小 bump?（design D6 倾向按反射特性分组）
