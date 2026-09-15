# Spec: 接口方法 static 保真度

## ADDED Requirements

### Requirement: 接口方法在 zbc/zpkg 中携带真实 static 位

#### Scenario: static abstract 接口成员导出与恢复
- **WHEN** 一个接口声明 `static abstract Self op_Add(Self, Self)` 并编译进 zpkg
- **THEN** 该方法在 zbc TYPE 段接口方法块的记录里 `is_static` 字节 = 1；
  下游包读回后 `ExportedMethodZ.IsStatic` = true、`MethodSymbol.IsStatic` = true

#### Scenario: 常规（实例）接口成员
- **WHEN** 一个接口声明 `int Compare(T a, T b)`（无 static）
- **THEN** `is_static` 字节 = 0；恢复后 `IsStatic=false`、`IsVirtual=true`、`IsAbstract=true`

### Requirement: 导入接口的满足性校验覆盖 static 种类

#### Scenario: 跨包正确实现 static-abstract 成员（放行）
- **WHEN** 下游包 `struct Money : INumber`（INumber 导入自 z42.core），以
  `public static override Money op_Add(Money a, Money b)` 实现全部 5 个 static 成员
- **THEN** 编译无 E0412（static 种类匹配、可见性 public、返回 Self≡Money 兼容）

#### Scenario: 跨包把 static 成员实现成 instance（报错）
- **WHEN** 下游包实现导入接口的 static-abstract 成员时**漏写 static**（实现成 instance 方法）
- **THEN** 报 E0412：`... is `static` in the interface and an instance method here`

## MODIFIED Requirements

### Requirement: `InheritanceResolver._checkOneIfaceMethod` 不再豁免导入接口

**Before:** 命中 MangleKey 后，`if (it.IsImported) { return; }` 跳过导入接口的
static/可见性/返回类型校验（因导入侧 `IsStatic` 恒 false，不跳则假红）。

**After:** 删除该守卫。导入接口与本包接口一样接受完整的 static/可见性/返回类型校验
（导入侧 `IsStatic` 已由 wire 修正为真实值）。

## IR Mapping

- zbc 1.41：TYPE 段接口方法块（`CLASS_FLAG_INTERFACE` gated）每方法记录新增
  `is_static:u8`，位置 `pcount` 后、`ptypes` 前。
- zpkg 0.46：内嵌 zbc 1.41（outer 布局不变）。

## Pipeline Steps

- [x] Lexer —（无）
- [x] Parser / AST —（无；`static` 修饰符已被 parser 解析进 `MethodDecl.Mods`）
- [ ] TypeChecker — `InheritanceResolver` 删守卫；满足性校验对导入接口生效
- [x] IR Codegen — `ClassDescBuilder` 填 static 平行数组；`ZbcWriter`/`ZbcReader`/`TsigReconcile` 搬运
- [x] VM interp — `type_reader.rs` 读 is_static 字节（派发行为不变）
