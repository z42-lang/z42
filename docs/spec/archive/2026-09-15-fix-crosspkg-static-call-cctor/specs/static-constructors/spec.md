# Spec: 静态构造器——跨包首次使用

## MODIFIED Requirements

### Requirement: 类型首次使用前执行其静态构造器（跨包同样成立）

四种首次使用（读 / 写静态字段、创建实例、调用静态方法）对**依赖包**里的类型与本包类型行为一致，与使用顺序无关。

#### Scenario: 跨包首次使用是调静态方法（方法体读静态字段）
- **WHEN** 依赖包 `Cfg` 有 `static int level = 2`、`static Cfg() { level += 40; }`、`static int Get() => level`；主包第一次使用是 `Cfg.Get()`
- **THEN** 打印 `42`（interp / JIT 相同）

#### Scenario: 跨包首次使用是调不碰任何静态字段的静态方法
- **WHEN** 依赖包 `Loud` 的静态 ctor 打印一行、`Ping()` 只 `return 7`；主包第一次使用是 `Loud.Ping()`，且此前没有任何其它 cctor 被登记
- **THEN** 先打印静态 ctor 那一行，再打印 `7`

#### Scenario: 结果与调用顺序无关
- **WHEN** 上两个场景以两种顺序组合
- **THEN** 两种顺序输出都与 C# 一致

#### Scenario: 静态属性访问器作为首次使用
- **WHEN** 依赖包类型有静态 ctor 与 auto 静态属性（带初始化器）；主包第一次使用是读该属性
- **THEN** 读到初始化器 / 静态 ctor 设定的值（恢复 #643 夹具的直接写法）

#### Scenario: 未被使用的依赖包类型不执行 cctor
- **WHEN** 依赖包被加载（因为用了其中别的类型），但带静态 ctor 的某类型从未被使用
- **THEN** 该 cctor 不执行（登记 ≠ 执行）
