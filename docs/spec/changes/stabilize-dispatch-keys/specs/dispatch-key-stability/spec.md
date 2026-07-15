# Spec: 派发键稳定性约定

## ADDED Requirements

### Requirement: 静态方法派发键是其自身签名的纯函数

**静态方法**的派发键（`RegKey`，用于调用点 emit、方法注册、zpkg 导出、跨包解析）**必须是该方法自身
签名的纯函数**，与同名兄弟方法的存在与否、数量、arity 无关。**实例方法**保持基线键规则不变（本变更
不改动实例/虚/接口/泛型/委托/foreach/ctor 派发）。

```
RegKey(m) =
    | MangleKey(m.name, m.paramTypes, m.paramCount)   当 m 为静态方法（恒 mangle）
    | m.name                                          当 m 为实例方法 且 IsProtocolExempt(m.name)
    | MangleKey(...)                                   当 m 为实例方法 且 同(name,arity)≥2 重载（type-based）
    | m.name "$" m.paramCount                          当 m 为实例方法 且 同名多 arity 重载
    | m.name                                           当 m 为实例方法 且 唯一（基线裸名）
MangleKey(name, types, n) = name "$" n ("$" TypeKey(types[i]))*
IsProtocolExempt ∈ { ToString, Equals, GetHashCode, GetType, get_Item, set_Item }
```

#### Scenario: 加静态重载不漂移现有静态键
- **WHEN** 一个原本唯一的**静态**方法 `Join(sep, arr)` 新增重载 `Join(sep, params string[])`
- **THEN** 其它静态方法的键不变；两个 Join 各得独立全签名键；已编译调用方不因兄弟集变化而失效

#### Scenario: 静态方法 emit == export == 注册
- **WHEN** 同一静态方法在调用点 emit、注册、zpkg 导出
- **THEN** 三处对该方法产生**同一字符串键**（全签名 mangle）

#### Scenario: prim 关键字静态调用命中 mangle 键
- **WHEN** 源写 `int.Parse(s)` / `string.FromChars(...)`（prim 包装类的静态方法）
- **THEN** 解析经 `_resolveOverload` 取 `RegKey`（如 `Parse$1$string`）emit，与注册函数名一致命中

#### Scenario: 实例方法键与本变更前逐字节一致
- **WHEN** 任一实例/虚/接口/泛型/委托/foreach/ctor 方法参与派发或 zpkg 导出
- **THEN** 其键、emit、vtable 槽、导出字节与本变更前**完全相同**（实例侧零行为变化）

#### Scenario: 协议豁免名保持裸键
- **WHEN** 方法名为 `ToString` / `Equals` / `get_Item` 等豁免名（实例）
- **THEN** 键为裸名，作为 VM/编译器/DepIndex 硬查锚点；反射 `MethodInfo.Name` 为源级名（静态方法去 `$`）

## MODIFIED Requirements

**Before:** `regName` 兄弟集相关（唯一→裸名 / 多 arity→`Name$arity` / 同 arity≥2→全签名），静态与实例
同规则。

**After:** **静态**方法恒全签名 mangle（切断兄弟集耦合，根治加/删重载破坏 bootstrap）；**实例**方法保持
基线兄弟集规则不变。VM vtable / ctor / 实例派发全不动。

## IR Mapping

无新 IR 指令 / 无 wire 布局变化。`CallInstr` / `VCallInstr` 的方法名操作数字符串、zpkg SIGS/导出方法
名随重键改变（内容变、布局不变）。zbc 1.27 / zpkg 0.32（strict-pin）。

## Pipeline Steps

- [x] TypeChecker / SymbolCollector（键规则：静态恒 mangle / 实例基线）
- [x] IR Codegen（静态测试索引用 RegKey；实例导出/impl 基线）
- [x] VM 元数据加载（vtable 槽键基线去 `$`）+ 反射展示去 `$`（静态方法名）
- [x] 格式版本（zbc 1.27 / zpkg 0.32）
