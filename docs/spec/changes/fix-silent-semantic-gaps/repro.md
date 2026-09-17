# 六条缺口的最小复现

> 在 `origin/main` = `b828b9cfe` 的干净 worktree 上实测（2026-09-17），种子走
> `./scripts/install-z42.sh` + `xtask build sdk`。运行方式：`./.z42/z42 run <file>`。
> 这些是后续回归用例（`src/tests/refs/` / `src/tests/types/` 等）的底稿。

## 缺口 1–3：静默错误（编译通过，结果不对）

```z42
using Std.IO;
void Inc(ref int x) { x = x + 1; }
struct One { public int x; public One(int x) { this.x = x; } }
class H { public int f; }
void Main() {
    var v = 1; Inc(v);                                   // 缺口 1：调用点漏写 ref
    Console.WriteLine("1) 漏写ref: " + v + " (应报错)");
    var arr = new int[1]; arr[0] = 10; Inc(ref arr[0]);  // 缺口 2a
    Console.WriteLine("2a) ref arr[0]: " + arr[0] + " (应 11)");
    var h = new H(); h.f = 20; Inc(ref h.f);             // 缺口 2b
    Console.WriteLine("2b) ref h.f: " + h.f + " (应 21)");
    var a = new One(1); var b = a; b.x = 99;             // 缺口 3
    Console.WriteLine("3) 单字段struct: " + a.x + " (应 1)");
}
```

实测输出：

```text
1) 漏写ref: 1 (应报错)          ← 编译通过、写入丢失
2a) ref arr[0]: 10 (应 11)      ← 写入丢失
2b) ref h.f: 20 (应 21)         ← 写入丢失
3) 单字段struct: 99 (应 1)      ← 值语义失效，两个名字共享同一份
```

### 缺口 3 的爆炸半径：stdlib 里恰好两个单字段 struct

全仓 struct 普查（`src/libraries/*/src` + `src/compiler/*/src`）：

| struct | 字段数 | 放宽后受影响？ |
|---|---|---|
| `Boolean` `Byte` `Char` `Double` `Int16/32/64` `SByte` `Single` `UInt16/32/64` | **0**（只有 static + extern，primitive-as-struct 机制） | 否 |
| `Guid` | **1**（`byte[] _bytes`） | ✅ 会翻成值语义 |
| `GCHandle` | **1**（`long _slot`） | ⚠️ **会翻，且有阻碍（见下）** |
| `KeyValuePair` | 2 | 否（已是 blob） |
| `ValueTuple2..8` | 2–8 | 否 |
| `ListEnumerator` | 2（`_list` + body `_pos`） | 否 |
| `DictionaryEnumerator` | 3（`_dict` + body `_scan` `_cur`） | 否 |

- **`Guid` 无 native**，单字段是 `byte[]` 引用、构造时已防御性复制、构造后不再变更
  ⇒ 翻成值语义**观察不到差别**，而且**顺带修掉** `Guid.z42:15-18` 自认的 `default(Guid)` 缺陷。
- **`GCHandle` 是真阻碍。** 它现在被 native 当**堆对象**处理：

  ```rust
  // src/runtime/src/corelib/gc.rs:190-196
  fn make_gc_handle(ctx: &VmContext, slot: u64) -> Value {
      ctx.heap().alloc_object(gc_handle_type_desc(), vec![Value::I64(slot as i64)], NativeData::None)
  }                                  // ← 返回 Value::Object

  // src/runtime/src/corelib/gc.rs:181-188
  fn extract_gc_handle_slot(arg: &Value) -> u64 {
      let Value::Object(rc) = arg else { return 0 };   // ← 只认 Value::Object
      ...
  }
  ```

  一旦 `GCHandle` 变成 blob 值 struct，native 收到的是 `Value::StructRef{...}`，
  那个 `else` 分支会**静默返回 0** ⇒ 每个句柄 `IsAllocated=false`，GC 句柄整体失效**且不报错**。

  ⚠️ **形态很讽刺：naive 地修这个静默 bug 会引入另一个静默 bug。**
  受影响的 builtin 共 5 个（`builtin_table.rs:257-261`）：`__gc_handle_alloc` / `_target` /
  `_is_alloc` / `_kind` / `_free`。缺口 3 若做，**必须同一个 change 里一并改这 5 个**，
  并补 GCHandle 的回归测试。

## 缺口 4：struct 的 static 字段读取即崩

```z42
using Std.IO;
struct Color { public int r; public int g;
  public Color(int r, int g) { this.r = r; this.g = g; }
  public static Color White = new Color(255, 255);
}
void Main() { Console.WriteLine(Color.White.r); }
```

```text
Error: uncaught exception: struct-value handle used after its creating frame exited
       (idx=0, frame_id=18) — value-struct lifetime unsound
```

### ⚠️ 判据比第一印象窄得多，也严重得多

第一印象是「struct 的静态字段坏了」。**实测否掉了这个描述**：

| 形态 | 结果 |
|---|---|
| `struct SHolder { public static int N = 7; }` | ✅ 打印 7 —— **struct 上的静态字段本身没问题** |
| `class Holder { public static Color White = ...; }` | ❌ **同样崩** |

⇒ 真判据是：**任何静态字段，只要它的类型是多字段值 struct，就坏**——
容器是 class 还是 struct 都一样。也就是说 `class Config { public static Point Origin = new Point(0,0); }`
这种极常见的写法同样崩。

### 存储矩阵：只有静态字段这一格是坏的

| 值 struct 存在哪 | 结果 |
|---|---|
| 局部变量 | ✅ |
| class 的**实例**字段（P3 堆内联，2026-08-11） | ✅ |
| `struct[]` 数组元素 | ✅ |
| 静态方法返回值（sret） | ✅ |
| **静态字段** | ❌ 崩 |

**实例字段当初正是用「把 blob 内联进堆对象」（P3）解决了同一个生命周期问题。
静态字段只是没走到那条路** —— 不是 arena 机制无解。

### 根因精确到一行

`src/runtime/src/interp/exec_object.rs:499-516` 的 `static_set` 把原始 `Value` 直接存进静态槽：

```rust
let v = frame.get(val)?.clone();
// add-escape-analysis-stack-alloc (diagnostic #2): StaticSet.val is an escape sink
debug_assert!(
    !matches!(v, Value::StackObject { .. } | Value::StackArray { .. }),
    "stack-alloc handle stored into a static field — escape analysis unsound (StaticSet.val)"
);
```

值 struct 存进去的是 `Value::StructRef { idx, frame_id }` —— **帧作用域的 arena 句柄**。
而那个 `debug_assert!` 拦了 `StackObject` / `StackArray` 逃逸进静态字段，
**却没拦 `StructRef`**，而它属于同一类 bug（`StackClosure` 同样没拦）。

⇒ 修法方向清楚：`static_set` 存之前要把 `StructRef` **提升**到堆/全局表示
（复用 P3 的堆内联，或装箱成 `BoxedStruct`），并把那条 `debug_assert!` 扩到
`StructRef` / `StackClosure`，让这类 bug 在 debug 构建里当场被抓。

## 缺口 5：struct 上的自动属性崩

```z42
using Std.IO;
struct P { public int X { get; set; } public int Y;
  public P(int x, int y) { this.X = x; this.Y = y; } }
void Main() { var p = new P(1, 2); Console.WriteLine(p.X); }
```

```text
Error: uncaught exception: struct ref leaf at byte offset 4294967295 not in type layout
  at P.P (line 3, col 28)
```

⇒ `4294967295` = `u32::MAX`：自动属性合成的后备字段**没有进 `StructLayout`**，
offset 保持未初始化值。崩在构造器里（`this.X = x` 这一行）。

## 缺口 6：重载集里子类实参不匹配用户基类形参

```z42
using Std.IO;
class A { public int a; }
class D : A { public int d; }
class C {
    public static void One(A x) { Console.WriteLine("One(A)"); }
    public static void F(A x) { Console.WriteLine("F(A)"); }
    public static void F(string s) { Console.WriteLine("F(string)"); }
}
void Main() {
    var d = new D();
    C.One(d);      // 单签名 → OK
    C.F(d);        // 两个重载 → 失败
}
```

```text
z42c build: 1 error(s)
  (12,5): E0401: no static method `F` on `C`
```

⇒ 只在第 12 行报错，第 11 行的单签名 `C.One(d)` **没有报错** ——
证实了「单签名走直接绑定、多签名才走适用性检查」这个不对称。
且报的是 `E0401`（找不到候选）而不是 `E0425`（候选歧义）。
