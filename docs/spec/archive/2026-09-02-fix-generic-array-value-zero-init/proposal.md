# Proposal: 泛型数组值类型零初始化根修（方案 C：显式操作数）

> 状态：DRAFT（待 User 确认）| 创建：2026-09-02 | 类型：lang/vm（zbc 格式 bump）

## Why

在**泛型方法/泛型类**里 `new T[n]`，当 T 绑到**值类型**（int/bool/char/double/值 struct）时，数组的
**未写入槽被初始化成 `Value::Null`，而非值类型的零值**（0/false/'\0'/零布局）。随后读该槽（尤其传给
取 `object` 形参的方法触发装箱，如 `Assert.Equal`）报
`Std.Exception: __box_prim: expected integer value, got Null`。见 [[generic-new-array-value-type-null-tail]]。

**根因**：z42 泛型是**类型擦除**（无单态化）。`new T[n]`（T 为形参）在 codegen emit `ArrayNewInstr`
时，T 被擦成 `elem_tag=Unknown(0x00)` + `elem_name="T"`（裸短名）；VM `array_new` 的
`default_value_for_tag(Unknown)` → `Value::Null`。VM **已具备**产正确零值的全部机制
（`default_value_for` + 运行期 `frame.method_type_args[idx]` / `Object.type_args[idx]`，即
`default(T)` 已走通的 `MethodDefault`/`DefaultOf` 路径），**唯一缺口**=`ArrayNewInstr` 不携带
「元素类型引用哪个类型参数」的索引，VM 无法把裸名 `"T"` 反查到具体类型参数（`method_type_args`
存的是**具体类型名**如 `["int"]`，不含形参名 `"T"`，故无法按名映射）。

**影响面广**：**任何泛型代码 `new T[n]` + 读未写值类型槽都会踩**，不止 `Array.Resize`（后者已用显式
`default(T)` 填尾绕过）。值得根修。

## What Changes（方案 C：显式操作数，User 2026-09-02 裁决）

给 `ArrayNewInstr` 加一个 **type-param 引用操作数**（kind: none/method/class + index），与 `default(T)`
的 `MethodDefaultInsn`/`DefaultOfInstr`（已用同款 ParamIndex 操作数）**同构**——这是最终数据模型，
无字符串哨兵约定、后续不返工。

- **IR / 格式**：`ArrayNewInstr` 新增 `TypeParamKind`（0=none / 1=method / 2=class）+ `TypeParamIndex`
  （kind=0 时 -1）。zbc writer/reader 对称加字段 → **zbc 1.37 / zpkg 0.42 格式 bump**。
- **codegen**（`ExprTyper._bindArrayNew` + `ExprEmitter`）：`new T[n]` 当元素是泛型形参时，复用
  `default(T)` 已有的形参归属+索引解析，emit `(kind, index)`；非泛型元素 emit `(0, -1)`。`ElemTag`
  仍走 `Unknown`（VM 以操作数为准）。
- **VM**（`exec_array.rs::array_new` + `bytecode.rs::ArrayNewInsn`）：读操作数，kind≠0 时按
  `frame.method_type_args[idx]`（method）/ 接收者 `Object.type_args[idx]`（class）解析出**具体类型名**，
  复用现成 `default_value_for` + `pack_backing` / `try_struct_backed` 产正确零值与 backing，并用具体
  类型名建 `ArrayObj.element_type`（**顺带修正泛型数组反射元素类型**：从擦除的 `"T"` 变具体 `Std.Int32`）。

**前置依赖已解除**（2026-09-02）：C 需格式 bump，此前受阻于 CI 两代自举格式-bump 回归；该回归已由
**#383（DepScanCache 陈旧缓存修复）合并 origin/main**，并经探针 #385（`compile-toolchain` 绿）复验。
故本 change 的格式 bump 走两代自举自动过，**无独立前置**。见 [[two-gen-bootstrap-regressed-blocks-format-bumps]]。

修完可移除 `Array.Resize` 的显式填尾绕过（次要收益）。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/libraries/z42.ir/src/IrInstr.z42` | MODIFY | `ArrayNewInstr` 加 `TypeParamKind` + `TypeParamIndex` |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcInstr.z42` | MODIFY | writer 编码新字段（格式 bump） |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReaderInstr.z42` | MODIFY | reader 对称解码 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | zbc Minor 36→37 + changelog |
| `src/libraries/z42.ir/src/ZpkgWriter.z42` | MODIFY | zpkg Minor 41→42 + changelog |
| `src/runtime/src/metadata/zbc_reader/versions.rs` | MODIFY | ZBC_VERSION_MINOR 37 / ZPKG_VERSION_MINOR 42 + changelog |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindArrayNew`：泛型形参解析 (kind, index) |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | ArrayNew emit 新操作数（:95 附近） |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `ArrayNewInsn` 加字段 + 反序列化 |
| `src/runtime/src/interp/exec_array.rs` | MODIFY | `array_new` 读操作数→查 type_args→具体零值+backing+element_type |
| `src/runtime/src/interp/exec_array_tests.rs` | MODIFY | VM 单测：操作数→值类型零值 |
| `src/libraries/z42.core/src/Array.z42` | MODIFY | 移除 Resize 显式填尾绕过 |
| `src/libraries/z42.core/tests/generic_array_zero_init.z42` | NEW | 端到端回归 |
| `docs/book/src/runtime/…（array/zbc 机制页）` | MODIFY | 零初始化机制 + 操作数编码（知识上浮） |
| `docs/spec/changes/fix-generic-array-value-zero-init/` | NEW | 本变更容器 |

> version-bumping.md checklist：zbc Minor bump 须同步 writer/reader/format 常量 + fixture 重生，实施时逐条核对。

## Out of Scope

- **`ArrayNewLitInstr`（`new T[]{...}`）**：所有槽被字面量写满，无 null-tail bug；不改其编码。
- **零-bump 哨兵方案（A）**——被否决（带日后返工，见 design Decision 1）。
- **reified generics**（运行期完整类型实参传递）——远大工程，不在本次。
- **JIT 路径**：M4 全绿前不填 JIT；`array_new` 的 JIT 版（如有）随后跟进或走 interp 回落，实施时确认。

## Open Question（唯一，待 User 定）

- **kind 编码宽度**：`TypeParamKind` 用 `u8`（0/1/2）够且直白；`TypeParamIndex` 用 `i32`（-1 哨兵）
  还是 varint？倾向 `u8 kind + varint index`（省字节、与既有 poolIdx 编码一致）。实施时定，不影响设计。
