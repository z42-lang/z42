# Spec: 硬转换的失败语义

## ADDED Requirements

### Requirement: 引用类型不匹配的硬转换抛 InvalidCastException

#### Scenario: 转成不相关的类
- **WHEN** `object o = new Other(); Box b = (Box)o;`
- **THEN** 抛 `InvalidCastException`，消息含源类型名与目标类型名，**可被 `catch (Exception)` 捕获**
- **注**：此前**不抛，把错类型的对象原样返回** —— 类型系统被静默绕过

#### Scenario: 转成基类合法
- **WHEN** `object o = new Derived(); Base b = (Base)o;`
- **THEN** 正常转换，不抛

#### Scenario: 转成已实现的接口合法
- **WHEN** `object o = new Impl(); IFoo f = (IFoo)o;`（`Impl : IFoo`）
- **THEN** 正常转换

#### Scenario: 跨包接口不误抛
- **WHEN** pkgA 导出 `IFoo` 与 `Impl : IFoo`，pkgB 写 `IFoo f = (IFoo)someObj;`
- **THEN** 正常转换 —— `is` 与 `as` 走同一个 `isa_td`，判定一致

### Requirement: null 的硬转换按目标类型分流

#### Scenario: null 转值类型
- **WHEN** `object n = null; int x = (int)n;`
- **THEN** 抛 `NullReferenceException`，**带源位置**，可 catch
- **注**：此前抛 `InvalidCastException: cannot convert Null to type tag 0x04` —— 内部错误串，
  `catch (Exception)` **抓不到**，且错误种类不对

#### Scenario: null 转引用类型合法
- **WHEN** `object n = null; string s = (string)n;`
- **THEN** 不抛，`s == null`（C# 同）

### Requirement: 类型不符的值类型硬转换抛 InvalidCastException

#### Scenario: 字符串转 int
- **WHEN** `object s = "hello"; int y = (int)s;`
- **THEN** 抛 `InvalidCastException`，消息用**用户可读的类型名**，可 catch
- **注**：此前 `cannot convert Str("hello") to type tag 0x04` —— Rust Debug 格式的内部错误

### Requirement: 静态可判时不产生额外指令

#### Scenario: 同类型 cast
- **WHEN** `int a = 1; int b = (int)a;`
- **THEN** 编译产物与本变更前 **byte-identical**（静态类型已等于目标 ⇒ 不发检查）

#### Scenario: 子类转基类
- **WHEN** `Derived d = new Derived(); Base b = (Base)d;`
- **THEN** 同上，不发检查

### Requirement: `as` 语义不变

#### Scenario: as 失配返 null
- **WHEN** `object o = new Other(); Box b = o as Box;`
- **THEN** `b == null`，**不抛**

#### Scenario: as 的指令序列不变
- **WHEN** 编译任何 `x as T`
- **THEN** 指令序列与本变更前 byte-identical
