# Design: 跨包 internal 类引用强制（类可见性序列化）

> 本 change 落地类级访问强制 ② 的**数据载体**。设计权威来自 ① design 的 **D5/D6**
> （`docs/spec/archive/2026-08-13-enforce-class-access/design.md`），此处原样承接并补实施细节。
> ① 已埋好强制**逻辑**（`AccessChecker.CheckTypeRef` 的 internal 分支：`t.IsImported && Visibility=="internal"`
> → E0404）；② 只补**可见性从声明到 importer 的序列化链**，让该分支对跨包 imported 类真正触发。

## Architecture

```
声明期  ClassDecl.Mods ──位置默认──> IrClassDesc.Visibility           [ClassDescBuilder]
                                        │
                                        ▼  zbc TYPE 记录：class_flags(u8) 后紧随 visibility(u8)
                                     ZbcWriter.WriteU8(cd.Visibility)
                                        │  ←→  ZbcReader.cd.Visibility = c.U8()
                                        ▼
                                     TsigReconcile: ecz.Visibility = _visStr(cd.Visibility)
                                        │
                                        ▼
                                     ImportedSymbolLoader: nct.Visibility = cl.Visibility
                                        │
引用期  绑定/收集期 CheckTypeRef(t, ...)  ◀── t.Visibility=="internal" && t.IsImported → E0404 (① 已实现)

Rust VM: zbc_reader.rs 读 visibility 字节 → read-and-discard（保 TYPE 后续偏移正确，不上反射）
```

**载体腿**（新增，本 change）与**逻辑腿**（① 已合）在 `Z42ClassType.Visibility` 汇合：① 已让本地类由
`SymbolCollector` 从 `Mods` 设 Visibility；② 补上 imported 类由元数据还原 Visibility。二者共用同一
`CheckTypeRef`。

## Decisions（承接 D5/D6，原样保留）

### Decision 5: 类可见性元数据 = TYPE 记录新增 `Visibility` 字节（真格式 bump）

**问题：** 跨包 internal 类判定需 importer 知被引类声明可见性。载体？

**事实：** ① 成员 internal=3 曾**零 bump**——因成员 `Visibility` u8 字段**早已存在**，只加值 3。② 类级**无**
任何现成可见性载体：`ExportedClassZ` 无 Visibility 字段；zbc TYPE 的 `class_flags` 是**已满 u8**
（bit0–7 全占，bit7=inline-struct，注释明写「last free bit」）——**塞不下**可见性位。

**选项：** A — 把 `class_flags` u8 拓宽为 u16 用 bit8–9；B — TYPE 记录新增独立 `Visibility` 字节（镜像成员）。

**决定：** 选 **B**（独立字节）。理由：与成员 Visibility 同构（成员用独立 int 而非塞 flags）、语义清晰
（可见性非 shape-flag）、拓宽 u8→u16 反而牵动更多字节。**真格式 bump**（zbc 1.32→1.33 / zpkg 0.37→0.38），
按 [version-bumping.md](../../../../.claude/rules/version-bumping.md) 全套步骤；与成员 internal 的零 bump
**不同**，不可类比。

**为何仍不破自举字节不动点：** 所有现存导出类均 `public`（Visibility=0）；新增字节对每个 TYPE 记录尾追加一个
`0` → z42c 自编译 gen1/gen2 同样追加、逐字节仍相等。fixture（含非导出/默认类）按格式 bump 常规重生。CI
`ci-bootstrap` 版本差 gate → 两代自举吸收（与近期多次真实 bump 同路径）。

### Decision 6: Rust VM 消费新字节但不上反射面

**问题：** VM 是否需要类可见性？

**决定：** VM **必须**读这个新字节（保持 TYPE 记录后续字段偏移正确），但 v1 **不**接入反射
（`Type.IsPublic` 等类级可见性反射列 Deferred，避免范围蔓延）。`zbc_reader.rs` 读它（`let _class_visibility
= c.read_u8()?`，read-and-discard）并 bump 两个版本常量；不 thread 进 `TypeDesc`（不接反射即读弃）。

## Implementation Notes

- **位置默认单一真相**：`ClassDescBuilder` 用 ① 已提供的 `IrGenFacts.classVisCode(mods, isNested)`（`+`名→private /
  顶层→internal / 显式修饰符优先），与 `SymbolCollector._putClassStub` 同源，避免漂移。`isNested` 判据 =
  `c.Name.IndexOf("+") >= 0`。
- **`ExportedTypeExtractor` 不改**：import 走 zbc `cd` → `TsigReconcile`（非本地 TSIG 提取），故类可见性还原
  只经 `TsigReconcile`；`ExportedTypeExtractor._extractClass` 是本地 TSIG 路径、不参与跨包还原。
- **`TsigReconcile._visStr`**：0→"public"/1→"private"/2→"protected"/3→"internal"；① 实现时已加 3→internal
  分支（供成员级），本 change 复用于类级。
- **字节顺序铁律**：writer `WriteU8(cd.Flags)` 后立即 `WriteU8(cd.Visibility)`；reader 严格对称
  `c.U8()` 顺序一致；Rust `read_u8()` 同位。任一处顺序错 = TYPE 后续字段（type-param/字段/内联 struct 块）
  全偏移错位。
- **默认最保守**：`IrClassDesc.Visibility`、`ExportedClassZ.Visibility` 均默认 public（0/"public"）——未显式设置
  的 desc 绝不误拒（strict-pin 保证同版本，缺字段不会发生）。
- **不动点自检**：CI 两代自举 `gen1==gen2`；任何字节漂移必是 public 类误 emit 非 0 可见性 → 查
  `classVisCode` 默认逻辑。

## Testing Strategy

- **单元（Rust）**：`zbc_reader_tests.rs` 版本 pin 32→33/37→38；`build_type_section_one_struct` 在
  `class_flags` 后 push 可见性字节 0，确认 struct 内联块解析仍对齐。
- **跨包 e2e**：`src/tests/cross-zpkg/class-internal-access/`——A 包 `internal class Secret`，B 包 `new Secret()`
  期望 E0404 `from another package`。若 cross-zpkg harness 无 expected-compile-error 模式（成员级即如此），则
  逻辑由 ① 单元 + 序列化链人工端到端验证覆盖，e2e 目录记录期望并手工验。
- **zbc golden**：`z42c.semantics/tests/zbc/zbc_tests.z42` 的 `empty/source.zbc` hex 重截（minor + TYPE 尾字节）。
- **格式 bump（CI 权威）**：`ci-bootstrap` 两代自举建 0.38；committed fixture（`src/tests/zbc-format/*`、
  `src/tests/zpkg-format/*`、`empty/source.zbc`）经**临时 CI 步**重生并回写。
- **GREEN**：完整 `xtask test` + `cargo test --lib`（**CI 上**——macOS 本地因两代自举墙不可产 0.38 stdlib）。
