# Spec: 派发键稳定性约定

## ADDED Requirements

### Requirement: 派发键是方法自身签名的纯函数

方法的**派发键**（`RegKey`，用于调用点 emit、方法注册、zpkg 导出、跨包解析、VM vtable 槽键）**必须
是该方法自身签名的纯函数**，与同名兄弟方法的存在与否、数量、arity 无关。

```
RegKey(m) =
    | m.name                                        当 m 为实例方法 且 IsProtocolExempt(m.name)
    | MangleKey(m.name, m.paramTypes, m.paramCount) 其余
MangleKey(name, types, n) = name "$" n ("$" TypeKey(types[i]))*
IsProtocolExempt ∈ { ToString, Equals, GetHashCode, GetType, get_Item, set_Item }
```

#### Scenario: 加重载不漂移现有键
- **WHEN** 一个原本唯一的方法 `Join(a,b)` 新增重载 `Join(params string[])`
- **THEN** `Join(a,b)` 的键仍为 `Join$2$string$string`（不变），新方法得独立键；已编译调用方不失效

#### Scenario: emit == export == vtable
- **WHEN** 同一方法在调用点 emit、注册、zpkg 导出、VM vtable 建槽
- **THEN** 四处对该方法产生**同一字符串键**

#### Scenario: 虚方法重载各占独立 vtable 槽
- **WHEN** 类有虚方法 `Foo(int)` 与 `Foo(string)`
- **THEN** vtable 槽键分别为 `Foo$1$int` / `Foo$1$string`，互不覆盖；VCall 按 `ms.RegKey` 精确命中

#### Scenario: 协议豁免名保持裸键
- **WHEN** 方法名为 `ToString` / `Equals` / `get_Item` 等豁免名（实例）
- **THEN** 键为裸名，作为 VM/编译器/DepIndex 硬查锚点；反射 `MethodInfo.Name` 亦为源级名（去 `$`）

## MODIFIED Requirements

**Before:** `regName` 兄弟集相关（唯一→裸名 / 多 arity→`Name$arity` / 同 arity≥2→全签名）；VM vtable
槽键在 `$` 处截断为裸名。

**After:** `regName` 一律全签名 mangle（豁免名除外）；VM vtable 槽键保留 `$`（= 全 mangle 键），与
VCall 操作数一致。

## IR Mapping

无新 IR 指令 / 无 wire 布局变化。`CallInstr` / `VCallInstr` 的方法名操作数字符串、zpkg SIGS/导出方法
名随重键改变（内容变、布局不变）。zbc 1.27 / zpkg 0.32（strict-pin）。

## Pipeline Steps

- [x] TypeChecker / SymbolCollector（键规则）
- [x] IR Codegen（导出 / impl / 测试索引键）
- [x] VM 元数据加载（vtable 槽键保留 `$`）+ 反射展示去 `$`
- [x] 格式版本（zbc 1.27 / zpkg 0.32）
