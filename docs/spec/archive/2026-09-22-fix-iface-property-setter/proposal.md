# proposal：fix-iface-property-setter

## 一句话

接口属性的 setter（`interface I { T P { get; set; } }` 里的 `set;`）此前被 `MemberCollector._fillInterface`
整段丢弃——只建 `get_P`、不建 `set_P`。补上 `set_P` 方法符号 + `AssignTyper` 的接口属性写分支，令接口
属性 setter 与**接口索引器 setter**（A3 `fix-iface-self-completeness-gaps` 已做）、**类属性 setter**
（`_fillClass` 已做）达到同等完整度。

## 背景：A3 补了索引器，漏了属性

`fix-iface-self-completeness-gaps`（第十三批）把接口**索引器**做到端到端——`_fillInterface` 为
`this[i] { get; set; }` 同时 lower `get_Item` + `set_Item`，`AssignTyper` 加了 `Z42InterfaceType`
收者的 `set_Item` 写分支。但**接口属性**（`P { get; set; }`）的 setter 半边一直漏网：

| 成员形态 | get 半边 | set 半边 |
|---|---|---|
| 类属性 `_fillClass` | ✅ get_P | ✅ set_P |
| 接口索引器 `_fillInterface`（A3） | ✅ get_Item | ✅ set_Item |
| **接口属性 `_fillInterface`** | ✅ get_P | ❌ **丢弃** ← 本 change |

## 症状（两个静默洞）

1. **满足性静默通过**：`interface I { int V { get; set; } } class C : I { public int V { get; } }`——`C` 只实现了
   getter，但因为接口的 `set_V` 契约从没进 `it.Methods`，满足性校验（`InheritanceResolver`）**看不到缺失**，
   编译静默通过。运行期经接口调 setter 才 `VCall not found`。
2. **经接口赋值静默丢弃**：`I x = c; x.V = 5;`——`AssignTyper` 的属性 setter 路径只认 `Z42ClassType`/
   `Z42InstantiatedType`，接口收者落空 → 落普通赋值路径 → `x.V` 绑成 `get_V` 调用、`_emitAssign` 认不出
   → **赋值被静默丢弃**（与 `fix-crosspkg-property-assign` 的类侧同族洞，接口侧漏网）。

## 改动（两处，镜像现成代码）

1. **`MemberCollector._fillInterface`**（收集侧）：`PropertyDecl` 分支在建 `get_P` 后，若 `pd.HasSet` 则补
   `set_P(value)` 方法符号（value 参类型 = 属性类型、void 返回、public）。镜像同函数下方的索引器 `set_Item`
   + 类侧 `_fillClass` 的 `set_P`。
2. **`AssignTyper`** 属性 setter 路径：新增 `Z42InterfaceType` 收者分支，从 `pIf.Methods` 解析 `set_P` 并发
   `instance` 虚调用（value 参按 setter 形参类型 box/convert）。镜像同文件索引器的 `set_Item` 接口分支。

**满足性校验自动生效**（零改动）：`set_P` 一进 `it.Methods`，`_checkOneIfaceMethod` 的 `MangleKey`
（名+形参，get_P/set_P 名不同 ⇒ 独立契约）自动要求实现方补齐 setter。

## 无格式 bump / 无发射漂移

纯编译期符号 + 绑定路径。`set_P` 只用于满足性校验与 setter 派发解析，本包接口的 `it.Methods` 不过 wire
（跨包接口属性 setter 是**另一个** Deferred，需格式 bump——见「边界」）。生产源码零接口属性 setter ⇒ 爆炸
半径 0，自举字节不动点由构造保证。

## 边界（本 change 不做）

- **跨包接口属性 setter**：本 change 只覆盖**本包**接口（同 A3 索引器初版）。导入接口属性的 set_P 保真需
  wire 承载（`ExportedInterfaceZ` 属性访问器元数据），是独立 Deferred，需格式 bump。本 change 对导入接口
  属性写不新增支持、也不新增假红。
- **get-only 接口属性的写**：写 get-only 接口属性（无 set_P）今天仍落普通路径静默丢弃——与类属性同款既有
  行为，非本 change 范围。
