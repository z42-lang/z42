# Proposal: 跨包 internal 类引用强制（类可见性进 zbc/zpkg 元数据）

> **承接（2026-08-13）**：本 change 是类级访问强制的**第二半 ②**，follow-up 自 `enforce-class-access`
> （①同包 private/protected 嵌套类，已合 #183）。①因**格式 bump 本地自举撞 macOS 两代自举墙**
> （见 memory `escape-stack-format-bump-ci-learnings`）经 User 裁决拆分：①（无格式 bump）本地完整 GREEN 先落地，
> **②（类可见性序列化 = 真格式 bump zbc1.33/zpkg0.38）走 CI 两代自举验证**。②的完整设计即 ① design 的
> **D5/D6**（`docs/spec/archive/2026-08-13-enforce-class-access/design.md`），本 change 承接并落地。

## Why

成员级访问强制（#180 / #181）+ 类级①（同包 private/protected 嵌套类，#183）已落地。但**跨包 `internal` 类**
仍不受约束——`internal`（含无修饰符顶层）类的可见性从未序列化进 zbc/zpkg 元数据，importer 无从得知被引类
的声明可见性，故 ① 里 `AccessChecker.CheckTypeRef` 的 internal 分支对 imported 类**永不触发**（imported 类
`Visibility` 默认 `public`）：

```z42
// 包 A
internal class Secret { }              // 默认 internal（顶层→模块）
// 包 B
void Use() { var s = new Secret(); }   // 跨包引用 A 的 internal 类 → 今天零诊断编译通过 ✗（应 E0404）
```

这是 default-member-private #181 与[语言规范](../../../design/language/access-control.md)承诺的**最后一块**。
封装在跨包类型层面仍形同虚设。本变更补上强制的**数据载体**：把类声明可见性序列化进 zbc TYPE 记录，
importer 还原后即激活 ① 已埋好的 internal deny 分支。

## What Changes

- **类可见性进 zbc/zpkg 元数据（真格式 bump）**：zbc TYPE 记录紧随 `class_flags` 新增**独立可见性字节**
  （0=public/1=private/2=protected/3=internal），镜像成员级 Visibility 独立字段。zbc 1.32→**1.33** /
  zpkg 0.37→**0.38**（`class_flags` 已满 u8，塞不下，故独立字节；非成员 internal=3 的零 bump，见 D5）。
- **序列化链贯通**：`IrClassDesc.Visibility`（源）→ `ClassDescBuilder`（从 `Mods` 位置默认设值）→
  `ZbcWriter`/`ZbcReader`（读写字节）→ `TsigReconcile`（cd.Visibility→`ExportedClassZ.Visibility`）→
  `ImportedSymbolLoader`（→`Z42ClassType.Visibility`）。importer 侧 `Visibility=="internal"` 即激活 ① 的
  `CheckTypeRef` 跨包 deny 分支（E0404 `... from another package`）。
- **VM 读而不用（D6）**：`zbc_reader.rs` 读这个新字节（保持 TYPE 记录后续偏移正确）但 **read-and-discard**——
  类可见性反射面（`Type.IsPublic` 等）列 Deferred，不接入 v1。
- **破坏面≈0**：z42c 235/235 + stdlib 生产 337/337 导出类全 `public` → 可见性字节恒 0 → 自举 gen1==gen2
  逐字节保持（每条 TYPE 记录仅尾追一个 `0`）。committed zbc/zpkg fixture 按格式 bump 常规重生。

## Scope（允许改动的文件）

### z42.ir（序列化载体）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/libraries/z42.ir/src/IrModule.z42` | MODIFY | `IrClassDesc` 加 `int Visibility`（默认 0） |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42` | MODIFY | TYPE 记录 `w.WriteU8(cd.Visibility)` 紧随 `WriteU8(cd.Flags)` |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | `cd.Visibility = c.U8()` 紧随 `cd.Flags = c.U8()` |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 32→33 + changelog |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 37→38 + changelog |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | `ExportedClassZ` 加 `string Visibility`（默认 "public"） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | `ecz.Visibility = _visStr(cd.Visibility)`（_visStr 已支持 3→internal） |

### z42c.semantics（本地设值 + 跨包还原）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | `cd2.Visibility = IrGenFacts.classVisCode(c.Mods, isNested)`（① 已提供 classVisCode） |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | `nct.Visibility = cl.Visibility`（跨包还原，激活 ① internal deny） |

### runtime（读而不用）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | TYPE 段 `let _class_visibility = c.read_u8()?`（read-and-discard）+ `ZBC_VERSION_MINOR` 32→33 + `ZPKG_VERSION_MINOR` 37→38 + changelog |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | 版本 pin 32→33/37→38；`build_type_section_one_struct` 在 `class_flags` 后 push 可见性字节 |

### 文档

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `docs/design/runtime/zbc.md` | MODIFY | Minor changelog 加 1.33 行 |
| `docs/design/runtime/zpkg.md` | MODIFY | Minor changelog 加 0.38 行 |
| `docs/design/language/access-control.md` | MODIFY | Status：跨包 internal 类强制已实现（移出 Deferred） |
| `src/tests/cross-zpkg/class-internal-access/` | ADD | 跨包 e2e：B 包引用 A 包 internal 类 → 期望 E0404 |

## Out of Scope（Deferred）

- **类可见性反射面**（`Type.IsPublic`/`IsInternal` 等类级反射）：VM read-and-discard，不接 v1（D6）。
- **顶层 private/protected 类的声明合法性**：真实代码无此写法（顶层类默认 internal），承 ① 一并 Deferred。

## 验证策略

- **本地（macOS，warm 0.37 上限）**：z42.ir gen0 编译（seed 0.37 编 ② 源过）+ `cargo build` + `cargo test --lib`
  （②新增 struct-byte / 版本 pin 单测过；committed 0.37 fixture 相关 5 测试 + 依赖本地 stdlib 的 host 集成测试
  因 0.38 格式本地不可产 stdlib 而失败——**格式-bump 本地不可验，属 macOS 两代自举墙，转 CI**）。
- **CI（权威 GREEN）**：`ci-bootstrap` 版本差 gate → **两代自举**建 0.38 全栈 + **临时 CI 步重生 committed
  fixture 并回写**（见 memory `escape-stack-format-bump-ci-learnings` 先例）→ 完整 GREEN → 合并后删临时步。
- **跨包 e2e**：`src/tests/cross-zpkg/class-internal-access/`——B 包引用 A 包 internal 类期望 E0404
  （若 harness 无 expected-compile-error 模式则手工验 + 记录，同成员级）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
