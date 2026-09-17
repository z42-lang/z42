# 参数修饰符 `ref` / `out` / `in`

> 对齐：2026-09-17 ｜ 实测基准：`./.z42/z42 run`

三个修饰符让方法改到调用方的变量，或按引用传递而不复制。**参数上和调用点上都必须写**——
z42 不像 C# 那样允许 `in` 在调用点省略。

| 修饰符 | 设计意图 | 调用点写法 |
|---|---|---|
| `ref T x` | 双向：调用前必须已赋值，方法可读可写，写入对调用方可见 | `f(ref x)` |
| `out T x` | 只出：调用前不必初始化，方法必须在返回前赋值 | `f(out v)` / `f(out var v)` |
| `in T x` | 只入：按引用传大值避免复制，方法不得写 | `f(in y)` |

> ## ⚠️ 当前实现不区分这三者
>
> **`ref`、`out`、`in` 在编译器里塌缩成同一个标志，行为一律等同 `ref`**，上表的"意图"
> 目前**没有任何编译期检查**兜底。实测（2026-09-17）：
>
> | 写法 | 应当 | 实际 |
> |---|---|---|
> | `void M(in int x) { x = 99; }` | 编译报错（`in` 只读） | 编译通过，调用方的变量被改成 99 |
> | `bool M(out int v) { return false; }`（从不赋 `v`） | 编译报错（`out` 必须赋值） | 编译通过；调用方读 `v` 抛 `__box_prim: expected integer value, got Null` |
> | `void Inc(ref int x)` 却调用成 `Inc(v)`（漏写 `ref`） | 编译报错 | 编译通过，**方法里的写入静默丢失** |
>
> 最后一条最容易咬人：漏写 `ref` 不报错，也没有任何运行期迹象，`v` 就是没变。
> **在这些检查补上之前，调用点请自己写全修饰符**，并把 `out` / `in` 当成纯粹的**可读性标注**。
>
> 本页下文凡标注"**未实现**"的规则，都属于同一个缺口。

## 语法

### 声明

```z42
// ref：双向
void Increment(ref int x) {
    x = x + 1;
}

// out：单向输出
bool TryParse(string s, out int v) {
    v = 42;
    return true;
}

// in：只读引用（意图），零拷贝传大值
double Norm(in BigVec v) {
    return Sqrt(v.x * v.x + v.y * v.y);
}
```

修饰符写在类型之前；`params` 之后。一个参数只能带一个修饰符。

### 调用点

```z42
var c = 0;
Increment(ref c);              // c == 1

if (TryParse("42", out var n)) {
    Console.WriteLine(n);      // `out var n` 内联声明，作用域延伸到 if 之后
}

var v = new BigVec(1.0, 2.0);
var d = Norm(in v);            // `in` 不可省
```

`out var x` 是唯一的内联声明形式（`ref var` / `in var` 无意义）。

### 实参必须是**局部变量**

```z42
Increment(ref c);           // ✓ 局部变量 —— 只有这一种真正写得回去
Increment(ref p);           // ✓ 本方法自己的参数（含它自己的 ref 参数，见"嵌套透传"）
```

### 实参必须是可取址的左值

能按引用传的有三种：

```z42
class Holder { public int f; }
class Wrap { public int[] a; }
void Inc(ref int x) { x = x + 1; }

int v = 0;              Inc(ref v);        // ① 局部变量 / 形参
int[] arr = new int[3]; Inc(ref arr[0]);   // ② 数组元素
var h = new Holder();   Inc(ref h.f);      // ③ 引用类对象的字段

var w = new Wrap(); w.a = new int[2];
Inc(ref w.a[0]);                           // ②③ 可以组合
```

下面几种**没有可取址的存储**，编译报 **`E0470`**：

| 写法 | 为什么不行 |
|---|---|
| `ref h.P`（属性 / 索引器） | 读走 getter、写走 setter，没有存储可取址 |
| `ref C.S`（静态字段） | 运行时没有对应的引用种类 |
| `ref p.x`（`p` 是值 `struct`） | 值类型的字节不在堆上，取址后无处写回 |
| `ref 42` / `ref F()` | 字面量、调用结果不是左值 |
| `ref arr.Length` | 虚成员，无存储 |

变通办法都是同一个：**读进局部变量 → 传局部变量 → 写回去**。

```z42
int tmp = h.P;
Inc(ref tmp);
h.P = tmp;
```

### 嵌套透传

`ref` 参数本身可以再作为 `ref` 实参传下去，写入会一路回到最初的调用方：

```z42
void Inner(ref int x) { x = x + 100; }
void Outer(ref int x) { Inner(ref x); }

var c = 1;
Outer(ref c);      // c == 101
```

### 与 `params` 互斥

```z42
void Foo(params ref int[] xs) { }    // ✗ E0208：'params' cannot combine with 'ref'/'out' or a default value
```

`params` 也不能与默认值同现。

## 多返回值：优先用元组

需要"返回多个值"时，**元组比 `out` 更合适**——它没有上面那些实现缺口：

```z42
(bool ok, int v) TryParse(string s) {
    return (true, 42);
}

var r = TryParse("42");
if (r.ok) { print(r.v); }
```

`out` 形式更接近 C# 风格，但当前只有"方法必须赋值"的承诺、没有强制。

## 不参与重载（未实现）

`Foo(int)` 与 `Foo(ref int)` **不是**两个重载：重载键只由方法名、参数个数与参数类型构成，
**不含修饰符位**，两者会撞成同一个键。

```z42
class C {
    public void Foo(int x) { }
    public void Foo(ref int x) { }   // ✗ E0408：duplicate overload `Foo`
}
```

顶层自由函数根本不参与重载（同名即 `E0408: free functions do not overload`），与修饰符无关。

## 尚未实现的规则一览

下面几条在设计上属于 `ref` / `out` / `in` 的完整语义，但当前编译器**一条都没有实施**。
写文档、写教程、做 code review 时不要把它们当作已生效的安全网。

| 规则 | 期望行为 | 现状 |
|---|---|---|
| 跨修饰符边界禁止隐式转换 | `long n; Foo(ref n)` 匹配 `Foo(ref int)` 时报错 | 未实现——编译通过并照常写回 |
| `out` 的 caller 端定值分析 | 调用后 `out` 实参视为已赋值（不再报"未初始化"） | 未实现——z42 本来就没有定值分析 |
| `out` 的 callee 端定值分析 | 方法在每条正常返回路径上都必须赋 `out` 参数 | 未实现——不赋值可编译，调用方读到 `Null` |
| `in` 写保护 | 方法体内对 `in` 参数赋值报错 | 未实现——`in` 完全等同 `ref` |
| 左值检查 | `ref 42` / `ref f()` 报错 | 未实现 |
| 修饰符参与重载 | `Foo(int)` 与 `Foo(ref int)` 是两个重载 | 未实现——撞键报 `E0408` |
| lambda 捕获禁止 | 捕获 `ref` 参数的 lambda 报错（引用不得逃出栈帧） | 未实现——可以捕获，按当时的值工作 |

**错误码**：上述检查尚未实现，因此**没有专用错误码**。相关的现有诊断只有
[`E0208`](../appendix/error-codes.md)（`params` 与修饰符冲突）与 `E0408`（重复重载）。

## 不支持的位置

修饰符**只能用在参数上**。下面这些 C# 形态 z42 一律不支持，也没有语法：

| 形态 | z42 |
|---|---|
| `ref` 局部变量 `ref int x = ref expr` | 无 |
| `ref` 返回类型 `ref T M()` | 无 |
| `ref` 字段 | 无 |
| `ref struct` 类型 | 无 |
| `scoped` 修饰符 | 无 |
| `ref readonly`（任何位置） | 无——参数位由 `in` 顶替 |

这是一条刻意的设计线：**引用永远不离开创建它的调用栈帧**，因此 z42 不需要生命周期标注、
不需要 `scoped`、也不需要借用检查器。泛型形参 `T` 同样不能绑定成 `ref T` 形态——修饰符不是类型。

## 与 C# 的对照

| C# 特性 | z42 | 说明 |
|---|---|---|
| `ref T` 参数 | ✓ | 语义一致 |
| `out T` 参数 | ✓ 语法一致 | 含 `out var x` 内联声明；定值分析**未实现** |
| `in T` 参数 | ✓ 语法一致，**调用点强制写** | 修正 C# `in` 可省的不一致；写保护**未实现**，当前完全等同 `ref` [^in] |
| `ref readonly T` 参数 | ✗ | 由 `in` 顶替 |
| `scoped` | ✗ | 引用不离开栈帧，用不上 |
| `ref T` 局部变量 / 返回 / 字段 | ✗ | 见上表 |
| `ref struct` 类型 | ✗ | GC 语言不需要 |
| `where T : allows ref struct` | ✗ | 随 `ref struct` 一起不存在 |
| 调用点 `in` 可省 | ✗ | 改为强制写，语法统一 |

[^in]: 也就是说 `in` 目前既不阻止方法写入，也不阻止写入传回调用方。把它读作"我不打算改"
的注释，而不是编译器给的保证。

## 相关

- [所有权与内存模型](memory-model.md)——值语义 / 引用语义的大局，以及什么时候才需要 `ref`
- [元组](tuples.md)——多返回值的首选形态
- [错误码](../appendix/error-codes.md)——`E0208` / `E0408`
