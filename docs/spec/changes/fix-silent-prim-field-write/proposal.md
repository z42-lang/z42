# 把「往基元字段写 Null」从静默无效改成报错

> 类型：**fix（vm 类 ⇒ 规范先行）**｜ 创建：2026-09-27｜ 状态：**已裁决 → IMPL**（2026-09-27 User 三条全按推荐通过）
> 出身：结构审计 2026-09 的 **R3**。审计原话：`Value::Null` 是**六义哨兵**，
> #822/#837/#838/继承字段 Null/`PrimModel` 静态表读成 Null「不是五个 bug，是一个表示缺陷的五张脸」，
> 且**已落地的修复全在读侧、写侧一处未堵**。本刀堵写侧那一处。

## 1. 实测：一处 `let _ =` 造成的用户可见静默失败

`ScriptObject::set_field_value`（`src/runtime/src/metadata/types/object.rs:336`）：

```rust
let _ = encode_prim(&mut self.bytes_mut(), fa.offset as usize, fa.width as usize, fa.tag, src);
false
```

`encode_prim` **本来就会拒绝** Null 写进任何基元槽
（`codec.rs::codec_as_i64` → `bail!("struct field: expected an integer value, got Null")`）。
问题是这里**把 `Result` 丢了**。

⭐ **判据是不对称本身**：全仓 `encode_prim` 有 **7 个调用点，6 个用 `?` 传播**
（`exec_struct.rs` ×5 + `accessors.rs:367`），**只有这一处 `let _ =`**。
不是「本仓的口径是容忍」，是这一处漏了。

### 实测五格（`--mode interp`，探针见下节）

| # | 路径 | 今天 | 该怎样 |
|---|---|---|---|
| ① | `FieldInfo.SetValue(heapObj, null)`，`int` 字段 | 🔴 **静默通过、不写入、报成功** | 抛 |
| ② | `FieldInfo.SetValue(boxedStruct, null)`，`int` 字段 | 抛 `Std.Exception` | 不变 |
| ③ | `FieldInfo.SetValue(obj, null)`，**对象内联 struct** 的基元叶子 | 抛 `Std.Exception` | 不变 |
| ④ | `PropertyInfo.SetValue(obj, null)`，`int` 自动属性 | 🔴 **静默通过** | 抛 |
| ⑤ | `Array.SetValue(int[], null, 0)` | 编译期 **E0475** 拦住 | 不变 |

⇒ **「应当抛」这个口径已由 ② ③ 两个兄弟确立**。本刀是把唯一的异类对齐，不是新立策略。

### 🔴 一个必须纠正的前提

很容易以为「Null 能到达基元字段写入点 ⇒ 说明编译器发错码了」，于是按 #837 的先例只加
`debug_assert`（debug bail / release 放行，理由是「那不是用户的错」）。

**④ 证明这个前提是错的**：`PropertyInfo.SetValue` 走的是**真正的 setter 方法调用**，
值经 **setter 内部编译器正确发射的 `FieldSet`** 落到同一处。
也就是说**普通合法 z42 代码经反射就能走到**，跟编译器有没有 bug 无关。

⇒ 所以**不能**只加 `debug_assert`：那会让 release 继续静默，而这是用户真实能触发的路径。
必须让错误在两个 profile 下都到达用户。

## 2. 复现探针（已实跑，非设想）

```z42
class Holder { public int N; public string S; }
// field(t, name) = 在 t.GetFields() 里按名字找 FieldInfo

field(t, "N").SetValue(h, 7);      // 基线：写得进
field(t, "N").SetValue(h, null);   // 🔴 静默：N 仍是 7，无异常
field(t, "S").SetValue(h, null);   // 对照：引用字段写 null **合法**，S 变 null
```

输出：

```
after ok set: N=7
after null set: N=7                       ← 🔴
after null on string: S is null = true    ← 对照必须保持
```

⚠️ **最后一行是这条修复的主要风险**：`string` / 对象 / 数组字段写 Null 是**完全合法**的
（走 `fa.ref_slot >= 0` 或 `TAG_OBJECT/TAG_ARRAY` 分支，都在 `encode_prim` 之前 return）。
修复绝不能碰那三条路。

## 3. 三个方案

### 方案 A（推荐）：`set_field_value` 保持不可失败，新增一个会失败的孪生方法

```rust
/// 与 `set_field_value` 同语义，但把基元编码失败**传出去**。
pub fn try_set_field_value(&mut self, slot: usize, v: &Value) -> anyhow::Result<bool>
```

`set_field_value` 变成它的包装：`self.try_set_field_value(slot, v).unwrap_or(false)`
—— 但**只留给确实不能失败的调用方**（零初始化、GC 测试、异常对象填栈迹：
那些的值由 VM 自己构造、类型必然对）。

把**能失败**的三条路改用 `try_`：
- `corelib/reflection/accessors.rs:198`（① 的现场）
- `interp/exec_object.rs` 的 `FieldSet`（④ 经由 setter 到达）
- `jit/helpers/object_field.rs` 的对应路径（**两个后端必须同时改**，否则就是
  「两个后端只有一个错」——本仓现有门禁最难发现的形态）

| 优点 | 缺点 |
|---|---|
| 调用点逐个**显式选择**「我这条路会不会失败」，而不是一刀切 | 多一个方法名 |
| 20 个既有调用点里绝大多数一字不改 ⇒ 回归面小 | 需要判断每个调用点归哪一类（判断依据写进注释） |
| 与既有 6 个 `?` 调用点同款 | |

### 方案 B：`set_field_value` 直接改成 `Result<bool>`

全部 ~20 个调用点都要动。零初始化 / GC 测试那些**不可能失败**的地方被迫写 `?` 或
`.expect()`，噪声大，且 `.expect()` 会把「不可能」变成 panic 风险。**不推荐**。

### 方案 C：只在 `accessors.rs:198` 前面加类型校验，不动 `set_field_value`

只修 ①，**修不了 ④**（④ 的现场在 IR 的 FieldSet，不经反射代码）。⇒ 半修，**不推荐**。

## 4. 裁决结果（2026-09-27，User 三条全按推荐）

| 裁决点 | 结论 |
|---|---|
| 修法 | **方案 A** —— `set_field_value` 保持不可失败，新增 `try_set_field_value -> Result<bool>` |
| 异常类型 | **对齐兄弟**，裸 `Std.Exception`（升级异常类型留给一刀专门收口三处） |
| 范围 | **不动 struct 侧**（`exec_struct.rs` 那 5 处已经 `?`） |

### 落地明细

`set_field_value` 变成 `try_set_field_value(..).unwrap_or(false)`，并在文档注释里写明
**它只许给「值由 VM 自己构造、类型必然对」的调用方**（零初始化 / GC 测试 / 往异常对象里
盖栈迹）。改用 `try_` 的是能收到**用户选择的值**的那三条：

| 路径 | 文件 | 处数 |
|---|---|---|
| 反射 `FieldInfo.SetValue` | `corelib/reflection/accessors.rs` | 1 |
| interp 的 `FieldSet` | `interp/exec_object.rs` | 4（栈对象 + PIC 命中 + PIC 装载 + 无 IC） |
| jit 的字段写 helper | `jit/helpers/object_field.rs` | 4（同上四格） |

其余 ~17 个调用点**一字未改**。

⚠️ jit 那侧不能用 `?`（helper 返回 `i32`：0 成功 / 1 已置异常），按它既有的 `set_exception`
范式写，且**先 `drop(b)` 再置异常** —— 别持着 `borrow_mut` 回调进 VM。

### 🔴 e2e 覆盖不到 jit 那半，已如实补 Rust 单测

写完 e2e fixture 后按铁律核了一遍 `Z42_JIT_PROFILE=1`：`--mode jit` 下只编了 **9 个函数**，
属性 setter `Holder.set_P` **不在其中** ⇒ jit 档那几行 "threw" **其实是解释器给的**，
`jit_field_set` 一次都没被执行。

⇒ jit 那半改由**直接打 helper** 的 Rust 单测覆盖
（`src/runtime/src/jit/helpers/object_field_tests.rs`，手法与同目录 `array_tests.rs` 一致，
那个文件的抬头记的正是同一条教训：「实测写出来的 e2e 用例在有 bug 的 VM 下照样全绿——等于没测」）。
fixture 的抬头注释也已改成如实说明它只覆盖 interp。

### 实测：阴性对照

把那一行改回 `let _ = encode_prim(..)`、其余一字不动：

| 用例 | 带修复 | 撤回后 |
|---|---|---|
| `null_into_primitive_field_raises_instead_of_silently_doing_nothing` | PASS | 🔴 **FAIL** |
| `type_mismatch_into_primitive_field_also_raises` | PASS | 🔴 **FAIL** |
| `null_into_reference_field_stays_legal`（对照） | PASS | **PASS** |

⇒ 恰好两条正例变红、对照保持绿 —— 分辨的是「真的把错误传出去了」与「把整条路堵死了」。

## 5.（原）需要 User 裁决的三点

1. **走方案 A 吗？**（我推荐 A）
2. **抛什么异常？** ② ③ 现在抛的是**裸 `Std.Exception`**（`bail!` 的默认落点）。
   两个选择：
   - **(a) 对齐兄弟**：也抛裸 `Std.Exception`。一致、零新增错误类型、最小改动。
   - **(b) 抛 `Std.InvalidCastException`**（C# 在这一格抛 `ArgumentException`）。
     语义更准，但那就该**连 ② ③ 一起换**，否则又造出新的不一致 —— 范围变大。
   我倾向 **(a)**，把「升级异常类型」留给一刀专门收口 ② ③ ④ 三处。
3. **要不要顺带堵 `struct` 侧的同形路径？** `exec_struct.rs` 那 5 处已经 `?` 了，
   所以 struct 侧没有这个洞 —— **我的建议是不动**，本刀只对齐 `object.rs` 那一处。

## 6. 验收判据（含阴性对照）

- **正面对照**（本仓铁律：全量零命中必须配正面对照，否则交付的是恒不响的门）：
  ① ④ 两格从「静默」变成「抛」，各一条 e2e + 各一条 Rust 单测。
- **回归门**：`string` / 对象 / 数组字段写 Null **仍然合法**（③ 的对照那一行），
  以及 `int` 字段写**正常整数**仍然写得进。这两条不加，修复可能以「把整条路堵死」的方式变绿。
- **两个后端**：interp + jit 各验一遍（jit 有独立的 `object_field.rs` 写路径）。
- `cargo test --lib`（**不带过滤**）+ `xtask test e2e` + `xtask test compiler`。
- **不需要格式 bump**（零 zbc/zpkg 字节变化）；**指纹也不需要**
  （编译器发码与诊断都不变，纯运行期行为）。

## 7. 不做

- 不改 ② ③ 的异常类型（见裁决点 2）。
- 不动 `exec_struct.rs` 的 5 处（它们已经 `?`）。
- 不碰 `Value::Null` 的表示本身。审计 R3 的根治是「给缺席/未初始化/void 各自专属通道」，
  那是大工程；本刀只堵写侧这一个已实测可达的洞。
