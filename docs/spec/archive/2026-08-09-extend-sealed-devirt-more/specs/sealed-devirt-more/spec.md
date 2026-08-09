# Spec: 去虚化扩到 sealed override + 泛型 sealed

## 概述

扩展 sealed 去虚化的触发条件，从「receiver 静态类型是非泛型 sealed 类」放宽到再覆盖：
① **sealed override**（非 sealed 类上的 sealed 方法）；② **泛型 sealed 类**。两者均满足「目标编译期唯一」，
去虚化为纯优化（结果与虚派发一致），`Opt.Devirt` 门控。不确定即回落 VCall，永不 miscall。

## Scenarios

### ① sealed override

**S1.1 非 sealed 类上的 sealed override → 去虚化**
```
class Base { public virtual int M() { return 1; } }
class Mid : Base { public sealed override int M() { return 2; } }
Mid m = new Mid(); m.M();     // → call @Mid.M（Opt.Devirt）；--no-opt → vcall
```
判据：declClass=Mid，`ms.IsSealed=true` → 目标唯一（Mid 之下无人能 override M）。

**S1.2 非 sealed override → 保持 vcall**
```
class Mid2 : Base { public override int M() { return 2; } }   // 未 sealed
Mid2 m = new Mid2(); m.M();   // → vcall（子类可再 override，目标不唯一）
```

**S1.3 子类继承 sealed 方法（多态安全）**
```
class Puppy : Mid { }         // 继承 Mid.M（sealed，不能 override）
Mid dp = new Puppy(); dp.M(); // → call @Mid.M；运行期 Puppy 用继承的 Mid.M → 正确
Puppy pp = new Puppy(); pp.M();// declClass 上溯到 Mid（sealed）→ call @Mid.M
```

**S1.4 sealed 方法之上的静态类型 → 保持 vcall**
```
Base b = new Mid(); b.M();    // declClass=Base，Base.M 非 sealed → vcall → 运行期 Mid.M
```

### ② 泛型 sealed

**S2.1 泛型 sealed receiver → 去虚化（arity-mangle 名）**
```
sealed class Box<T> { public virtual T Get() { ... } }
Box<int> b = new Box<int>(); b.Get();   // → call @Box$1.Get（$N mangle）；--no-opt → vcall
```

**S2.2 非 sealed 泛型 → 保持 vcall**
```
class Bag<T> { public virtual T Get() { ... } }
Bag<int> g = ...; g.Get();     // → vcall（非 sealed，可继承）
```

## 正确性铁律

- **不确定即回落**：任何目标名不可精确构造 / 声明点 sealed 判据不成立 → `""` → VCall（虚派发永远正确）。
- **`Opt.Devirt` 门控**：`--no-opt devirt` 对拍验证纯优化等价。
- **自举不动点**：z42c 自身源大量用 sealed / sealed override → 改自编译输出 → 当次 gen1≠gen2（D7），warm 重建自愈至 gen1==gen2。

## 非目标

- 非虚方法 / 接口 receiver / cast-unknown 链（既有守卫优先）。
- 数据流型别精化（`if (x is T)` 后窄化）——仍按静态声明类型。
