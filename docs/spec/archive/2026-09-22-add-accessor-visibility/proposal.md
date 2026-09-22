# proposal：add-accessor-visibility

## 一句话

属性访问器级可见性修饰符（`public int P { get; private set; }` 里的 `private`）此前被**语义层完全忽略** +
**IR 恒发 public** ⇒ `private set` 被静默当 public：外部能写、反射报告 public。让它真生效——外部写 private
setter 报 E0404、反射报真实可见性、封装成立（对标 C# `{ get; private set; }`）。

## 症状（两层丢失，都有活跃受害者）

1. **语义层丢**：`MemberCollector._fillClass` 给 get_X/set_X 符号统一用**属性级** `pvis`，从不读 `pd.SetMods`/
   `pd.GetMods` ⇒ `private set` 的 set_X 被注册成 public ⇒ 外部 `obj.P = v` 编译通过。
2. **IR 层丢**：`IrGenMemberEmitter` 发 auto-prop / 计算体 accessor 时**不设 `.Visibility`**（对比普通方法
   `IrGen:40` 有）⇒ `IrFunction.Visibility` 默认 0=public ⇒ 反射 `GetProperty`/accessor 恒报 public，
   连属性级 private/protected 都丢。

**活跃受害者**：`z42.test/Bencher.z42`（8 个 `{ get; private set; }`）、`z42.test/TestIO.z42`——私有 setter
对外可见可调、反射报错 public。

## 改动（4 处）

1. **`MemberCollector._fillClass`**：get_X 符号 vis = `_vis(pd.GetMods, pvis)`、set_X = `_vis(pd.SetMods, pvis)`
   （访问器级覆盖属性级；空则回落，`_vis` 对空 mods 回落 dflt）。
2. **`IrGenMemberEmitter.EmitProperty`**：5 个 accessor 发射点（计算 getter / extern getter / auto getter /
   auto setter / 自定义 setter）都设 `.Visibility`（getter 用 GetMods→属性、setter 用 SetMods→属性）。
3. **`FieldSymbol`** 加 `SetterVis`（属性 setter 可见性；空=与属性级同）。`MemberCollector` 在 `pd.SetMods != ""`
   时设。
4. **`AssignTyper`** 属性写路径：`fs.IsProp && !fs.IsPropNoSetter && fs.SetterVis != ""` 时对 `SetterVis` 做
   `CheckAccess` → 外部写 private/protected setter 报 **E0404**。

## 为什么需要第 3/4 处（本地类属性的写限制）

访问控制强制不是「符号可见性一改就自动生效」：`AssignTyper` 现有的 set_X `CheckAccess` 路径**只对导入类/接口**
（无 FieldSymbol）。本地类属性以**源名 FieldSymbol** 登记（`ct.Fields[P]`），写 `c.P = v` 走 FieldSymbol 路径、
查的是 FieldSymbol.Visibility（属性级=public）。故须把 setter 可见性带到 FieldSymbol（`SetterVis`）、在写属性时
额外 `CheckAccess`。

## 无格式 bump / 爆炸半径

`Visibility` 是既有 zbc 字段（无格式 bump）。`private set` 强制**可能**新拒外部写，但全仓预扫 + 清建实测**零命中**
（Bencher/TestIO 的 private-set 属性只类内 `this.X=` 写）⇒ 爆炸半径 0。IR accessor vis 字节变化由测试 harness
golden 自动重生吸收（无 committed golden 改动）。自举不动点 3/3（seed→gen1 accessor vis 字节变=修生效，gen1==gen2）。
