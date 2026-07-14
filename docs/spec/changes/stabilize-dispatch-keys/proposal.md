# Proposal: 派发键稳定化（方案 A）+ `Path.Join` / `String.Join` 落地 `params`

## Why

`params` 的「支持」早在 `add-params-varargs`（2026-07-01）就 GREEN，但动机 API
`Path.Join` / `String.Join` 至今无法真正改用 `params`——每次尝试都撞同一堵墙，最终在
`compiler_review.md §2.4` / `roadmap.md §350` 记为「派发键 bootstrap 敏感性」，**最终方案 A** 延后。
本变更把方案 A 落地，并同一次格式 bump 内让 `Path.Join` / `String.Join` 用上 `params`。

### 根因（实测确认）

调用派发走 mangle 名键：`SymbolCollector.regName` 给方法算 `RegKey` →
`BoundCall.MethodName = ms.RegKey` → emit `CallInstr/VCall("<FQ>.Join$2$string$string", …)` →
VM 按**字符串名精确查找**。`regName` 规则**兄弟集相关**（唯一→裸 `Join`；多 arity→`Join$2`；同
(name,arity)≥2→全签名）。于是**给唯一方法加一个重载 → 它从裸键 re-mangle**（`Join`→`Join$2`），
288 处已编译调用方（含 9 个 z42c 源文件 + 预编译 seed driver）仍指向旧裸键 → 打新库
`undefined Std.IO.Path.Join`。且纯键变更不 bump 格式 → ci-bootstrap 两代自举不触发 → 不自愈。

## What Changes

- **方案 A —— 派发键一律全签名 mangle**：`regName` 恒用 `OverloadResolver.MangleKey`（键 = 方法自身
  签名纯函数、与兄弟无关）。实例协议豁免名（`ToString`/`Equals`/`GetHashCode`/`GetType`/`get_Item`/
  `set_Item`）保持裸名（VM vtable / DepIndex 硬查锚点）。→ 键永久稳定，未来加/删重载零 bootstrap。
- **单一真相键**：`ExportedTypeExtractor` / `IrGen`(impl) / `TestIndexBuilder` 统一优先 `md.RegKey`
  （消 P3-3 重复 mangle 规则）；`DependencyIndex` 实例查找注册完整键。
- **VM vtable 键保留 `$`**：`derive_simple_method_name` 不再截断 `$` → vtable 槽键 = 全 mangle 键，
  与 VCall 操作数（`ms.RegKey`）一致；重载虚方法各占独立槽。反射 `MethodInfo.Name` 反向去 `$` 展示。
- **格式 bump（User 裁决双 bump）**：zbc 1.26→1.27 + zpkg 0.31→0.32（writer + Rust reader 同步）。
  wire 布局不变，仅字符串内容随重键变；bump 触发 ci-bootstrap 版本差 gate → 两代自举整树重键。
- **`Path.Join(params string[])` 新增（保留 2-arg）；`String.Join(string, params string[])` 取代
  `Join(string, string[])` + 3-固定-arg**（params string[] 与 string[] 同签名不可并存，合一）。

## Scope（允许改动的文件 / 子系统）

占用子系统锁：`compiler` + `ir` + `runtime` + `stdlib`（本会话独立分支，直接推进）。

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | `regName` 恒 MangleKey；删兄弟集预扫描 |
| `src/compiler/z42c.semantics/src/ExportedTypeExtractor.z42` | MODIFY | 实例方法键优先 `md.RegKey` |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | impl 方法键优先 `imd.RegKey` |
| `src/compiler/z42c.semantics/src/TestIndexBuilder.z42` | MODIFY | 测试方法键优先 `md.RegKey` |
| `src/compiler/z42c.ir/src/DependencyIndex.z42` | MODIFY | 实例查找注册完整键 |
| `src/compiler/z42c.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `ZbcVersion.Minor` 26→27 |
| `src/compiler/z42c.project/src/ZpkgWriter.z42` | MODIFY | `ZpkgWriterZ.Minor` 31→32 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `derive_simple_method_name` 保留 `$` |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `MethodInfo.Name` 去 `$` 展示 |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | reader minor 常量 27 / 32 + changelog |
| `src/runtime/src/metadata/zbc_reader_tests.rs` | MODIFY | version-pin 断言 27 / 32 |
| `src/libraries/z42.io/src/Path.z42` | MODIFY | `Join(params string[])` |
| `src/libraries/z42.core/src/String.z42` | MODIFY | `Join(string, params string[])` |
| `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` | MODIFY | golden hex minor 1a00→1b00 |
| `src/tests/zbc-format/*/source.zbc` | MODIFY | header minor 版本-patch（CI 自动重键覆写） |
| `src/tests/zpkg-format/*/source.{zpkg,zbc}` | MODIFY | outer minor + indexed 内 zbc minor + hash 版本-patch |
| `docs/design/runtime/{zbc,zpkg}.md` | MODIFY | Minor changelog + 当前版本 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记/释放锁 |

## 验证现实（冷环境）

本地无 seed、nightly 下载被代理 403 → **自举链路 / golden regen 本地不可跑**，只能
`cargo build` + Rust 单测本地验，其余 **push 后盯 CI**（`ci-bootstrap` 两代自举 +
`bootstrap-no-csharp` + golden regen + z42c 不动点）。方案 A 是「push 盯 CI」型变更。
