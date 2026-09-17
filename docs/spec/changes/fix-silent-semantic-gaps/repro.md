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

⇒ 注意 `frame_id=18`：静态初始化在某个帧里建了 blob，帧退出后句柄就悬空。
**arena 是 per-frame LIFO，静态字段却是模块级生命周期** —— 这是机制层面的矛盾，不是小 bug。

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
