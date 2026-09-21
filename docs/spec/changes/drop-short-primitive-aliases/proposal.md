# Proposal: 删除 primitive 短名别名，源码与 IR 拼写统一到 C# 关键字

## Why

z42 的每个内建标量类型都有**两套源码拼写**：C# 关键字（`byte`）与 Rust 风格短名（`u8`）。
[types.md](../../../reference/src/language/types.md) 把这当特性记着，甚至写「**短名是规范形式**」。
代价是：

| 层 | 现状 | 问题 |
|---|---|---|
| ① 源码类型拼写 | `byte` 与 `u8` 都能写 | 同一个类型两个名字 ⇒ 重载 / 诊断 / 反射处处要决定「显示哪个」；读代码的人要记两套 |
| ② IR / zbc canonical 名 | `int/long/float/double` 走关键字，`byte→u8`、`sbyte→i8`、`short→i16`、`ushort→u16`、`uint→u32`、`ulong→u64` 走短名 | **同一张表里两套风格**。由此长出 `_isAliasPrim` 守卫（`FunctionEmitter` / `ClassDescBuilder` 各一份：SIGS/TYPE 要保源拼写，故把 byte/sbyte/short/ushort/uint/ulong 从规范化路径上摘出去） |
| ③ FFI ABI 签名语言 | `[Extern]` 签名串里的 `u8` / `usize` / `*const T` / `CStr` | **这层不动**——它是 Rust/C ABI 记法（[native-interop 参考页](../../../reference/src/embedding/native-interop.md)明写 `u8 → uint8_t`），不是 z42 类型拼写 |
| ④ IR 文本 dump | `add i32 %1, %2`（`IrType.Name`） | **这层也不动**——LLVM 风格的 IR 汇编记法 |

短名的实际使用量很小：全仓 `i32`/`i64`/`f32`/`f64` 作为源码拼写 **0 处**，在用的只有
`i8/i16/u8/u16/u32/u64` 共 160 行 / 31 文件，且 `src/compiler` 里**一处都没有**。
留着它等于让语言表面积白白翻倍。

删 ② 与删 ① 是一件事的两半：canonical 之所以是短名，只因历史上 `_canonPrim` 把次要关键字归一过去。
统一到关键字后 **源拼写 == canonical == 线格式名**，两份 `_isAliasPrim` 守卫同结果、成为纯噪音，
已整个删除——这是本次吃 format bump 换来的实际收益。

### 分层是正常的，同层两个名字才是冗余

C# 自己就是三套拼写：源码 `int` / CIL `int32` / 元数据 `System.Int32`。z42 保留 ③④ 的短名与此同形。
要消除的从来不是「不同层用不同记法」，而是**同一层里同一个类型有两个名字**。

### 为什么这不违反「不做兼容」

删短名是**收窄**语言表面，不是加兼容层。且不需要两-nightly 分阶段：上一版 nightly 的 z42c 一直认
`byte/sbyte/...`，源码改写后它照编；z42c 自身停止接受短名与源码改写可同一 commit 落地
（种子编的是**源码**，而源码已不含短名）。

## What Changes

1. **源码层删短名**：`src/libraries` 的 160 处 `i8/i16/u8/u16/u32/u64` 改写为
   `sbyte/short/byte/ushort/uint/ulong`（z42.ir 12 文件、z42.core 11、z42.crypto 4、z42.net 2、z42.io 1、z42.compression 1）。
2. **z42c 停止接受短名拼写**：`SymbolTable._isPrim` 去掉短名分支；短名出现在源码里应报「未定义类型」。
3. **IR / zbc canonical 统一到关键字**（= format bump）：`PrimModel.Keyword`、`SymbolTable._canonPrim`、
   `TypeNameResolver._canonName`、`EmitContext`、`TypeFactsTc` 一律产出 `sbyte/short/byte/ushort/uint/ulong`；
   两份 `_isAliasPrim` 守卫删除（源拼写与 canonical 合流）。
4. **runtime 跟随**：`struct_reflect.rs` 的 keyword→短名映射表变恒等、match 分支改关键字；
   `array.rs` / `reflection/type_object.rs` / `reflection/generics.rs` 的短名 arm 删除。
   **注**：这几处**本来就把短名归一到关键字**（`"sbyte" | "i8" | "Std.SByte" => "sbyte"`），
   故用户可见的反射行为**零变化**，变的只有 zbc 串池里的字节。
5. **派发键随之变化**（本次 bump 的隐藏维度）：`OverloadResolver.TypeKey` 走 `CanonName()`，
   故非-primary 重载的 mangle 从 `Substring$2$i32$i32` 变成 `$int$int`。同族先例 = zbc 1.38
   `stabilize-dispatch-keys`。这让自举比纯格式 bump **多需要一代**（见 tasks.md 阶段 6.5）。
6. **格式 bump**：zbc minor + zpkg minor，按 [version-bumping.md](../../../agent/rules/version-bumping.md) checklist 走。
7. **`[Extern]` / FFI ABI 层不动**（③）：`dispatch.rs::parse_type` 的 `i8/u8/usize/CStr` 记法原样保留。
8. **文档**：naming-conventions.md 修冲突；interop.md 补一句「FFI 签名是 C ABI 记法，与 z42 源码类型拼写是两套」；
   docs/book 对应机制页记录「源拼写 == IR canonical」这一新不变式。

9. **编译器内部 canonical key 一并收敛**（User 裁决 2026-09-21）：`PrimModel.Canon` 的返回值从
   短名（`"i32"` / `"u8"`）改为关键字（`"int"` / `"byte"`）。于是 `Canon == Keyword`，
   **`PrimModel.Keyword` 整个删除**，调用点改调 `Canon`；编译器内部不再出现 `i8/i32/u8` 字符串
   （`IrTag` 的分支、~87 处字面量比较点同步改）。收敛后「任意拼写 → 唯一关键字」只剩一张表。

## 不做（本 PR）

- API 对称化（TryParse / MinValue / MaxValue / Single.Parse / …）走 PR-B `symmetrize-primitive-api`。

## Scope（允许改动的文件）

| 路径 | 变更类型 | 说明 |
|---|---|---|
| `src/libraries/**` (31 文件) | MODIFY | 短名 → 关键字拼写 |
| `src/compiler/z42c.semantics/src/{PrimModel,SymbolTable,TypeNameResolver,EmitContext,TypeFactsTc,FunctionEmitter}.z42` | MODIFY | canonical 统一 + 停止接受短名 + 删 `Keyword()` |
| `src/compiler/**`（~87 处短名字面量比较点） | MODIFY | 内部 key 收敛到关键字 |
| `src/libraries/z42.ir/src/{ZpkgWriter,BinaryFormat/ZbcFormat}.z42` | MODIFY | 格式 minor bump |
| `src/runtime/src/corelib/{struct_reflect,array}.rs`, `src/runtime/src/corelib/reflection/*.rs` | MODIFY | 短名 arm 删除 |
| `src/runtime/src/metadata/**` | MODIFY | strict-pin 版本常量 |
| `docs/design/language/{naming-conventions,interop}.md` | MODIFY | 规范冲突修正 + FFI 层澄清 |
| `docs/book/src/compiler/*.md` | MODIFY | 记录新不变式 |
| 相关 tests / goldens | MODIFY | 断言与 fixture 刷新 |

## 验证

- `xtask test` 全绿（含 `test lines` / `test bootstrap`）
- 自举字节不动点：gen1 == gen2
- 负例：源码写 `u8 x = 1;` 必须报未定义类型（自带 fixture，防「新诊断触发 0 次」）
- 反射行为退回对照：`typeof(byte).Name` 改动前后一致
