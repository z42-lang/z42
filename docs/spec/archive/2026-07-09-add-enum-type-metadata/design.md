# Design: enum 类型元数据（P1-a）

## Architecture

enum 从"编译期常量映射"升为"TYPE 段类型实体",三处联动:

```
z42c: EnumDecl ──IrGen──▶ IrClassDesc{ isEnum, members:[(name,i64)] }
                              │ ZbcWriter.BuildType
                              ▼
        TYPE 段每类: ... class_flags(bit5=enum) ...
                     若 enum: +member_count:u16 +(name_str_idx:u32, value:i64)×n   ← 追加块
                              │
        Rust read_type ───────▼──▶ ClassDesc{ class_flags, enum_members }
                              │ → TypeDesc{ is_enum, enum_members }
                              ▼
        反射: Type.IsEnum / Std.Enum.GetNames/GetValues/GetName
        typeof(Direction) → 该 enum 的 Type 实体（因 TYPE 现有 enum 条目）
```

**不改 enum 的值模型**:运行时 enum 值仍是 int（`Direction.North`==0）。TYPE 的 enum 成员块只承载
"name↔value 映射"供反射;`GetValues` MVP 返 `int[]`。强类型 enum 值延后。

## Decisions

### D1: enum 成员块追加在 TYPE 每类记录末尾（additive）

现 TYPE 每类:name/base/fields/static_fields/interfaces/flags/type_params/constraints/attributes。
enum 用 `class_flags bit5=enum` 标记,**若该 bit 置位**则在类记录**末尾**追加:
```
member_count: u16
count × { name_str_idx: u32, value: i64 }
```
非 enum 类不写此块 → 老 zbc 布局对普通类**字节不变**（只是新增 bit5 语义 + enum 专属尾块)。
strict-pin 下 regen 即可。

### D2: enum 也是"类"——复用 ClassDesc/TypeDesc，最小侵入

enum 进 `classes` 列表（作特殊 class),不新开顶层段。`ClassDesc` 加 `enum_members: Box<[(String,i64)]>`
（非 enum 为空)。`TypeDesc.is_enum = (class_flags & FLAG_ENUM)!=0`。这样 typeof/GetType/反射
全走现有类型注册表,零新机制。enum 无字段/方法/vtable → 其余块空。

### D3: typeof(EnumType) 解析——enum 进类符号表

现在 enum 只在 `EnumConsts/EnumTypes`（常量解析),不在类符号表 → `typeof(Direction)` 无 Type。
P1-a:`SymbolCollector` 让 enum 也登记一个类符号（`Z42ClassType` 标 enum),使 `typeof`/`GetType`
解析到它。**常量映射保留**（`Direction.North`→0 的编译路径不变),只是额外多一个类型实体。

### D4: 反射 API 面（z42.core，对齐 C# 子集）

- `Type.IsEnum`（属性）:读 `TypeDesc.is_enum`。
- `Std.Enum.GetNames(Type) -> string[]`:enum 成员名。
- `Std.Enum.GetValues(Type) -> int[]`:enum 成员值（MVP int）。
- `Std.Enum.GetName(Type, int) -> string`:值→名（未命中返 null/空）。
builtin 从 `TypeDesc.enum_members` 读。非 enum Type 调这些 → 抛/空(对齐 C# `ArgumentException`,
MVP 可返空 + 文档注明)。

### D5: TSIG enum 块 P1 不动（双份并存）

P1 阶段 TYPE 加 enum,TSIG 的 enum 块**照旧**（z42c 跨包解析仍读 TSIG)。这是 initiative 三阶段的
必然:P1 让 TYPE 成超集,P2 对账,P3 才删 TSIG。故本 change **不碰** z42c 跨包 enum 解析路径,
零回归风险。

## Implementation Notes

- `IrClassDesc`（z42c IR）加 `IsEnum:bool` + `EnumMembers`（并行 name[]/value[]）。`IrGen` 从
  `EnumDecl` 填（值:显式 `= N` 或隐式递增,复用现有 enum 常量求值逻辑)。
- `InternPoolStrings` 预扫:enum 成员名须入 STRS 池(供 name_str_idx),位置镜像现类段预扫。
- `BuildType`:写 class_flags 时置 enum bit;若 enum,写成员块。**顺序**:成员块放在类记录既有
  尾部之后（attributes 之后),保 additive。
- Rust `read_type`:读到 enum bit → 读成员块;否则 enum_members 空。
- 反射值:MVP `int[]`——`Enum.GetValues` builtin 构造 int 数组（element_type "int"）。

## Testing Strategy

- **z42c 单测**:enum 类 BuildType golden hex（含 flag + 成员块）；typeof(Direction) 解析。
- **Rust 单测**:read_type 读 enum 成员往返;TypeDesc.is_enum。
- **端到端**（`src/tests/types/enum_reflect.z42`):`typeof(Direction).IsEnum==true` +
  `Enum.GetNames(typeof(Status))` == ["Ok","NotFound","ServerError"] +
  `Enum.GetValues(typeof(Status))` == [0,404,500] + `GetName(typeof(Status),404)=="NotFound"`。
- **z42.core [Test]**:Enum API 三件 + IsEnum 对普通类=false。
- 全 GREEN + 自举不动点 + 两代自举 format bump（本地端到端 + CI verify-selfhost）。
- 对账:enum 常量编译路径（`Direction.North`==0）不回归(现有 enum golden 保持)。
