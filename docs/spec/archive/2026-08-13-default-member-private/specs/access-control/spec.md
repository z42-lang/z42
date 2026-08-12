# Spec: Default Member Visibility = private

## MODIFIED Requirements

### Requirement: 无修饰符成员默认 private

**Before:** 无修饰符类成员默认 `internal`（同包跨类可访问）。
**After:** 无修饰符类成员默认 `private`（仅声明类文本内），镜像 access-control.md「最小封闭作用域」。

#### Scenario: 无修饰符成员跨类访问 → E0404
- **WHEN** `class A { int a; }`，另一类 `B` 经 `A` 实例访问 `a`
- **THEN** emit `E0404`

#### Scenario: 无修饰符成员同类内访问 → 通过
- **WHEN** 在声明类内经 `this` 访问无修饰符成员
- **THEN** 无诊断

#### Scenario: 无修饰符自由函数默认 internal → 同包调用通过
- **WHEN** 顶层无修饰符自由函数被同包另一处调用
- **THEN** 无诊断（自由函数封闭层=模块）

## ADDED Requirements

### Requirement: 组合访问修饰符 → E0405

#### Scenario: protected internal → 编译错误
- **WHEN** 声明写 `protected internal` / `private protected`（2+ 访问修饰符）
- **THEN** emit `E0405`「cannot combine access modifiers」

## IR Mapping

无新增指令。`_visCode` 无修饰符成员 → int `1`(private)（自由函数 → `3` internal）。既有 u8 值域不变，
**无格式 bump**；成员 vis 字节 3→1 是元数据 delta，被自举 gen1==gen2 吸收。

## Pipeline Steps

- [x] Parser（`_parseModifiers` 组合修饰符 E0405）
- [x] TypeChecker（`_vis` 位置默认 → AccessChecker）
- [x] IR Codegen（`_visCode` 位置默认）
- [ ] VM interp — 无
