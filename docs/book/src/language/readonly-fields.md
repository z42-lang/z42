# readonly 字段

> 对齐：2026-08-06（change `add-readonly-fields-opt`）

`readonly` 修饰符标记一个**构造后不变**的字段。它既是给读者的意图声明，也是给优化器的
**可信契约**——编译器据此把该字段的读（`field_get`）纳入 CSE 消重与循环外提（LICM），详见
[优化管线 · pass 2f](../runtime/optimization-pipeline.md#机制--实现)。

## 语法

```z42
class Vec {
    readonly int x;
    readonly int y;
    public Vec(int a, int b) {
        this.x = a;   // ✓ ctor 内经 this 赋值
        this.y = b;
    }
    public int SumSq() {
        return this.x * this.x + this.y * this.y;   // 只读，随处可读
    }
}
```

字段初始化器等价于 ctor 内赋值：

```z42
class Config {
    readonly int limit = 256;   // ✓ 初始化器（注入合成/显式 ctor）
}
```

## 不变性规则（类型检查强制）

readonly 字段**只能**在以下两处赋值，其余任何位置赋值报 **`E0415`**：

1. **声明类的实例构造函数体内**，且经 `this.<field> = ...`；
2. **字段初始化器**（`readonly T f = ...;`，由编译器注入 ctor）。

```z42
class C {
    readonly int x;
    public C(int a) { this.x = a; }        // ✓
    public void set(int a) { this.x = a; } // ✗ E0415：ctor 外赋值
    public void bare(int a) { x = a; }     // ✗ E0415：裸写（隐式 this）也算
}
class D {
    readonly int x;
    public D(D other) { other.x = 1; }     // ✗ E0415：只允许 this.<field>，不允许别的对象
}
```

## 语义与优化

- readonly 是**引用不可变的字段槽**：字段本身构造后不再被赋值。它不深度冻结所指对象
  （若字段是引用类型，被指对象的内部仍可变——与 C# `readonly` 一致）。
- 优化收益（同模块）：一个方法反复读自己的 readonly 字段时，
  - **块内 CSE**：`this.x + this.x` 只读一次；
  - **LICM**：循环体内 `this.x`（接收者 `this` 恒非空）提到 pre-header，每次进循环只读一次。
  - 实测热循环 interp **~1.87×**（`src/libraries/z42.core/bench/readonly_field_bench.z42`）。

## 当前边界（Deferred）

- **跨 zpkg 导入字段**：v1 只识别**同模块**声明的 readonly（跨包需 zbc/zpkg 格式 bump 把 readonly
  位写进字段元数据）——导入字段保守当作非 readonly（不误优化，只是少优化）。
- **非 `this` 接收者的 LICM 外提**（形参 / 局部变量的 readonly 字段读）：需非空 / 支配分析证明无
  NPE 时机漂移，留待非空类型系统。
- `readonly struct` / `readonly` 参数：各自独立特性。

## 关联文档

- 机制 / 优化：[优化管线](../runtime/optimization-pipeline.md)（pass 2f）
- 引入：change `add-readonly-fields-opt`（`docs/spec/archive/`）
