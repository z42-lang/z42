# Design: 模式匹配 A3（or-模式带绑定）

## Architecture

A3 **不新增编译器阶段、不改数据流、不动 syntax 层**——parser 早在 A2 就产出带绑定的 `OrPattern`。
差异全部落在 semantics 的两个环节：`PatternBinder._bindOr`（绑定收集 + 一致性校验）与 `PatternEmitter`
的 or lowering（合流）。

```
源码  case Circle(r) | Square(r) if r > 10:
  │  syntax（PatternParser —— A2 已支持，零改）
  ▼  OrPattern[ PositionalPattern(Circle, [Name r]), PositionalPattern(Square, [Name r]) ]
  │  semantics（PatternBinder._bindOr —— A3：各 alt 绑进子作用域、收集绑定集、校验一致、统一注册）
  ▼  BoundOrPattern{ Alts, BindNames=[r], BindTypes=[int], BindCount=1 }
  │  semantics（PatternEmitter —— A3：BindCount>0 → phi-free 合流）
  ▼  既有 IR：stable_r=Alloc(int)
        alt0: IsInstance(Circle)→field_get r0→okL0: Copy(stable_r, r0)→matchL
        alt1: IsInstance(Square)→field_get r1→okL1: Copy(stable_r, r1)→matchL
        matchL: Locals[r]=stable_r → 守卫 r>10 → arm body
```

## Decisions

### Decision 1: 绑定集一致性 —— 各 alt 绑**同名同类型**，类型必须完全相同（User 裁决）

各 alt 必须绑定**完全相同的变量集**（同名），且同名变量的**类型完全相同**（按 `Z42Type.Name()` 相等）。
不一致 → 报 `TypeMismatch` 诊断：

- `Circle(r) | Square(s)` → 名字集不同（`{r}` vs `{s}`）→ 报错。
- `Circle(r) | Triangle` → `Triangle` 无绑定、`{r}` vs `{}` → 报错。
- `Circle(r:int) | Square(r:double)` → 同名不同类型 → 报错。

**为何要求类型完全相同**（而非取 LUB / 公共基类）：与当前极简实现风格一致；LUB 算法是独立复杂度，
留待有实际需求再放宽（记入 Out）。Rust 亦要求 or 各 alt 绑定同一类型。

**实现**：每个 alt 绑进 `env.PushScope()` 的**独立子作用域**（绑定落 `child.Vars`，不污染 env、不互相
串味）；`child.Vars.Keys()` 收集该 alt 绑定集；首个 alt 作参考集，后续 alt 逐一集合式比对（名字存在性 +
`Name()` 相等）。校验通过后，统一绑定集一次性 `env.Define` 进真实 env（供守卫 / arm body 类型检查）。

### Decision 2: phi-free 合流 —— 稳定寄存器 + `CopyInstr`

z42 IR **无 phi 节点**；A1/A2 的绑定是**零成本别名**（`Locals.Put(name, existingReg)`）。or 各 alt 把同名
变量绑到**不同**寄存器（Circle 的字段读 vs Square 的字段读），别名无法表达「合流」。

**解**：为每个统一绑定预分配一个**稳定寄存器** `stable[k]`；各 alt 匹配成功后，在自己的 `okL` 落地块把
该 alt 绑的变量 `Copy` 进 `stable[k]`，再跳 matchL。matchL 处所有 alt 都已把值搬进同一 `stable[k]` →
`Locals[name] = stable[k]`（单一、一致）。

```
BindCount>0 lowering（伪代码）：
  for k in 0..BindCount: stable[k] = Alloc(ToIrType(BindTypes[k]))
  for i in 0..Count:
     okL = Fresh("pat_or_ok"); failNext = (i<last ? Fresh("pat_or") : failL)
     EmitMatch(subj, Alts[i], okL, failNext)     // alt 成功→okL；失败→failNext
     StartBlock(okL)
       for k in 0..BindCount: Copy(stable[k], Locals.Get(BindNames[k]))
       EndBlock(Br(matchL))
     if i<last: StartBlock(failNext)              // 下一 alt 从 failNext 起
  for k in 0..BindCount: Locals.Put(BindNames[k], stable[k])   // matchL 处可见
```

**关键不变量**：`okL_i` 的 Copy 紧跟 `EmitMatch(Alts[i])` 之后、下一 alt 的 `EmitMatch` 之前发射 →
`Locals.Get(name)` 此刻正是**本 alt** 绑的寄存器（下一 alt 尚未覆盖）。

### Decision 3: 递归可组合（嵌套 or 带绑定，User 裁决支持）

or 可作**子模式**出现（positional / property 元素经 `_parsePattern` 解析，含 or-链）——
`Box(Circle(r) | Square(r))`。A3 的合流**天然递归可组合**，无需特判嵌套：

- 内层 or 先把 `r` 合流进**它自己的**稳定寄存器 `inner_stable`，并 `Locals.Put(r, inner_stable)`。
- 内层 or 的 matchL = 外层 positional 的 `nextFieldL`；到达时 `Locals[r] = inner_stable`（单一寄存器）。
- 若外层也是带绑定 or，其 `okL` 的 `Copy(outer_stable, Locals.Get(r)=inner_stable)` 读到单一寄存器 → 正确。

**不变量**：`EmitMatch(alt)` 到达其 matchL 时，每个 or-绑定名都解析到**单一、一致**的寄存器——嵌套 or
已先合流成自己的稳定寄存器，故此不变量对任意嵌套深度成立。

### Decision 4: `BindCount==0` 严格 byte-identical

无绑定 or（= A2 的**全部**用法：多常量 `1|2|3`、多类型 `Cat|Dog`、多区间）走**逐字未改**的旧
lowering 分支（依次尝试、失败落下一 alt、末 alt 失败落 failL）。A3 的合流分支仅在 `BindCount>0` 生效。
→ A2 的任何现有用法 emit 不变；z42c 源无 or-模式 → 自举不动点无影响。

## 数据结构

```
BoundOrPattern（BoundPattern.z42）:
  BoundPattern[] Alts
  int            Count
  string[]       BindNames   // 统一绑定集名字（声明序 = 首个 alt 的 child.Vars.Keys() 序）
  Z42Type[]      BindTypes   // 对应类型（各 alt 一致）
  int            BindCount   // 0 = 无绑定 or（A2 形态）
```

`BindNames` 的顺序取自首个 alt 的 `child.Vars.Keys()`——各绑定的 Copy 相互独立，顺序不影响正确性；
仅影响 IR 中独立 Copy 指令的相对次序，且带绑定 or 只在测试文件出现（无 golden、不入自举不动点）。

## 边界 / 坑

- **jit 安全**：合流用 `CopyInstr(stable, src)`（值搬运），不涉 `as_cast`+`field_get`（record 程序 jit 误编
  教训）。字段读仍走 A1 的 `FieldGetInstr` 直读。→ interp / jit 双验。
- **@ in or**：`v @ >0 | v @ <0` 各 alt 绑 `{v}`。@ 绑定别名到 subj（同一寄存器），Copy 冗余但无害。
- **裸绑定 or**：`x | y`（两裸名）绑定集 `{x}` vs `{y}` 不一致 → 报错（且裸绑定恒真、第二 alt 不可达，本就病态）。
- **守卫可见性**：统一绑定在 or lowering 完成后 `Locals.Put`，守卫 / arm body 发射在其后 → 正常可见。
  binder 侧 `env.Define` 让守卫 / arm body 类型检查也看到绑定。
