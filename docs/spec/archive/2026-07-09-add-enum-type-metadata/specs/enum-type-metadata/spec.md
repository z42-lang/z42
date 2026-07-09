# Spec: enum 类型元数据

## ADDED Requirements

### Requirement: enum 作为 TYPE 段类型实体

#### Scenario: enum emit 进 TYPE + class_flags
- **WHEN** z42c 编译 `enum Status { Ok = 0, NotFound = 404, ServerError = 500 }`
- **THEN** TYPE 段含一个类 `Status`,`class_flags` bit5(enum) 置位,追加成员块
  `count=3, (Ok,0)(NotFound,404)(ServerError,500)`

#### Scenario: 普通类字节不受影响
- **WHEN** z42c 编译一个普通 `class Greeter { ... }`
- **THEN** 其 TYPE 记录不含 enum 成员块,class_flags bit5=0(布局与 pre-P1a 一致,仅 regen 版本号变)

#### Scenario: typeof(EnumType) 解析到 Type 实体
- **WHEN** 运行 `typeof(Status)`
- **THEN** 得到非 null 的 Type,其 `IsEnum == true`

#### Scenario: 反射枚举成员
- **WHEN** `Std.Enum.GetNames(typeof(Status))` / `GetValues(typeof(Status))`
- **THEN** 分别得 `["Ok","NotFound","ServerError"]` / `[0, 404, 500]`（顺序 = 声明序）

#### Scenario: 值→名
- **WHEN** `Std.Enum.GetName(typeof(Status), 404)`
- **THEN** 得 `"NotFound"`；未命中值 → null/空

#### Scenario: 普通类 IsEnum=false
- **WHEN** `typeof(Greeter).IsEnum`
- **THEN** `false`

#### Scenario: enum 常量编译不回归
- **WHEN** 代码用 `Status.NotFound`
- **THEN** 仍编译为 int 常量 404(现有 enum 常量路径 + golden 不变)

#### Scenario: 跨包 enum 解析不回归（P1 双份）
- **WHEN** A 包 enum,B 包 `using A; ... A.Status.Ok`
- **THEN** 仍经 TSIG enum 块解析(P1 不碰跨包路径),行为不变

#### Scenario: strict-pin 版本
- **WHEN** 旧 minor reader 遇新产物
- **THEN** 按 strict-pin 拒绝(regen 后新 reader 正常)

## IR Mapping

无新 opcode。TYPE 段每类记录条件追加 enum 成员块(class_flags bit5 gated)。

## Pipeline Steps

- [ ] Lexer — 不涉及
- [ ] Parser / AST — 不涉及(EnumDecl 已有)
- [x] TypeChecker/SymbolCollector（enum 进类符号表供 typeof）
- [x] IR Codegen（IrGen: EnumDecl→IrClassDesc(enum)；ZbcWriter.BuildType 成员块）
- [x] VM interp（read_type enum 块 + 反射 builtin）
