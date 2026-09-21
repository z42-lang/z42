# proposal：add-property-custom-setter

## 一句话

补齐类属性的**自定义 setter**（`T P { get { ... } set { ... } }` / `set => e;`）——此前只支持计算 getter
（`get { ... }`）+ auto setter（`set;`），带体 setter `set { ... }` 被 parser 当类型解析、报 `E0443: undefined
type: set` 级联。让属性 setter 与**计算 getter**（`add-property-getter`）、**索引器 get+set 体**达到同等完整度。

## 背景：不对称的洞

| 成员形态 | getter 体 | setter 体 |
|---|---|---|
| 类属性 | ✅ `get { }`（计算 getter） | ❌ `set { }` **parser 炸**（当类型）← 本 change |
| 索引器 | ✅ `get { }` | ✅ `set { }` |

`PropertyDecl` 只有 `HasGetBody`/`GetBody`，无 `SetBody`；parser `_parseProperty` 的带体分支只对 `get` 开，
`set { }` 落 auto-property 的 `_expectSemi()` → 期望 `;` 遇 `{` → setter 体 token 全被误当访问器/类型 → 级联。

## 改动（7 处，全镜像计算 getter / 索引器 set）

1. **`PropertyDecl`**（Decl.z42）：加 `HasSetBody`/`SetBody`（镜像 `HasGetBody`/`GetBody`）+ Dump。
2. **Parser `_parseProperty`**：带体分支泛化到 `get`+`set`（`set { }` / `set => e;`，setter 体 void）。
3. **混合访问器诊断 E0474**（parser，字面量发码同 E0449–E0473 族）：两个访问器都在时须**要么都 auto、
   要么都带体**——z42 无 C# `field` 关键字，auto 半边（读/写 `__prop_X`）与带体半边（自管存储）会读写错位。
4. **`MemberCollector`**：`pfHasStorage = !extern && !HasGetBody && !HasSetBody`（任一带体 ⇒ 无 `__prop_X` 后备）。
5. **`DeclBinder`**：`HasSetBody` → 绑 `set_X` 体（env 含隐式 `value` 参=属性类型 + `this`，镜像索引器 set_Item）。
6. **`IrGenMemberEmitter.EmitProperty`**：`HasSetBody` → 发真实 `set_X` 函数（链外单发，因 `{ get{} set{} }`
   时 HasGetBody 走首分支、auto 链跳过）。
7. **`ClassDescBuilder`**：后备字段条件加 `&& !HasSetBody`（自定义 setter 无 `__prop_X`）。

满足性/写解析（`set_X` 方法符号、`IsPropNoSetter`、AssignTyper 写路径）**零改动自动生效**（HasSet=true）。

## 语义（User 裁决 2026-09-22）

- **完整支持**自定义 setter（不是只做干净报错）。
- **混合访问器报错**（E0474），不允许 `{ get; set {} }` / `{ get {} set; }`。
- 全手动属性（`{ get {} set {} }`）**无 `__prop_X` 后备**，类自备字段。

## 无格式 bump / 自举纪律

纯编译期 AST + 绑定/发射路径，无 zbc/zpkg 格式变更。z42c/stdlib 源**本轮不使用**自定义 setter
（bootstrap-seed 轴①：support 先行、use 晚一 nightly）⇒ 上一 nightly 能编当前源，`test bootstrap` NO violation。
