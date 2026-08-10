# Spec: struct→object 健全装箱 + 身份

## ADDED Requirements

### Requirement: blob 值 struct 擦除到 object 时装箱到堆稳定表示

blob 值 struct 赋给 `object` / 接口（变量 / 参数 / 数组元素 / 返回值）时，编译器插入装箱——把帧作用域
arena blob **拷贝**进堆 `Value::BoxedStruct`（拥有 bytes+refs 副本 + 类型名），生命周期脱离创建帧。

#### Scenario: 装箱值跨帧存活（修悬垂）
- **WHEN** 某函数 `object Make() { var p = new P(1,2); return p; }` 返回 struct 到 object；调用方在
  `Make` 帧退出后持有该 object
- **THEN** 访问该 object（`GetType` / `is` / 拆箱）**不崩溃、不 stale**（blob 已拷进堆，非帧 arena 句柄）

#### Scenario: 装箱是值快照
- **WHEN** `var p = new P(1,2); object o = p; p.x = 99;`
- **THEN** `((P)o).x == 1`（装箱时快照，后续改原 struct 不影响已装箱副本）

### Requirement: boxed struct 的运行期类型身份

#### Scenario: GetType
- **WHEN** `object o = new P(1,2); var t = o.GetType();`
- **THEN** `t` 是 struct 类型 `P`（type_name 驱动）

#### Scenario: is
- **WHEN** `object o = new P(1,2);`
- **THEN** `o is P` → `true`；`o is object` → `true`；`o is SomeOtherStruct` → `false`

#### Scenario: as / 显式拆箱
- **WHEN** `object o = new P(1,2); P q = (P)o;`（或 `o as P`）
- **THEN** 得到值 struct（堆 blob 拷回当前帧 arena StructRef），`q.x == 1 && q.y == 2`；拆箱后 `q` 与
  `o` 独立（改 `q.x` 不影响再次 `(P)o`）
- **WHEN** `object o = new P(1,2);` 而目标是不匹配类型 `Q`
- **THEN** `o as Q` → `null`（引用类型 as 失败语义）

### Requirement: boxed struct 的 GC 可达性

#### Scenario: 引用叶子存活
- **WHEN** boxed struct 含 `string`/object 引用叶子，且该 boxed 值是活根
- **THEN** GC 扫描 `BoxedStruct.refs`，引用叶子被标记存活（不误回收）

## MODIFIED Requirements

### Requirement: struct→object 赋值的运行时行为

**Before:** `object o = someStruct` 裸拷 `Value::StructRef` 句柄进 object 槽；帧退出后经 `o` 访问 =
use-after-free；`is`/`as`/`GetType` 无 struct 分支 → 答错。

**After:** 编译器在擦除点插入 `__box_struct` 装箱 → 堆 `BoxedStruct`（值快照，GC 扫描）；`is`/`as`/`GetType`
识别 `BoxedStruct` 并按类型名正确回答；`(P)o` 拆箱回值 struct。

## IR Mapping

- **不新增 opcode / 不 bump 格式**（复用现有 `Builtin` 0x51）：
  - 装箱 = `ConstStr(structFQ)` + `Builtin(dst, "__box_struct", [structHandle, cls])`
  - 拆箱 = 现有 `AsCast`（0x72）扩 VM 分支（BoxedStruct→arena StructRef）
  - 身份 = 现有 `IsInstance`(0x71)/`Typeof`+`GetType`(VCall/builtin) 扩 VM 分支

## Pipeline Steps

- [ ] Lexer / Parser / AST —— 无
- [x] TypeChecker —— `BoxIfNeeded` 扩（blob struct 擦除→BoundBox struct kind）
- [x] IR Codegen —— `_emitBox` 对 struct 发 `__box_struct`
- [x] VM interp —— `Value::BoxedStruct` + `builtin_box_struct` + is/as/GetType/convert(拆箱)/trace 分支
