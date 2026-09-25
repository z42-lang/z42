# Spec: 泛型擦除槽按实例化算布局

## ADDED Requirements

### Requirement: 型参实参是 blob struct 时，字段槽内联该 struct 的字节

泛型类型 `G<…>` 的字段若声明类型为型参 `T`，而该实例化把 `T` 绑到一个 **blob struct** `S`，
则 `G<…>` 这个**实例化**的布局中该字段是 `S` 的**内联字节**（`StructLeafKind.Struct`，
size = `S` 的 size），而非 8 字节 `GcRef` 句柄。

#### Scenario: 泛型 struct 的 T 槽（形态 ①）
- **WHEN** `struct P2 { int X; int Y; }`，`struct Pair<A,B> { A First; B Second; }`，
  执行 `P2 inner; inner.Y = 2; var pp = new Pair<P2,int>(inner, 1); inner.Y = 99;`
- **THEN** `pp.First.Y == 2` —— 存入时 `inner` 的字节被拷进 `pp` 的 blob，
  此后改 `inner` 与 `pp` 无关

#### Scenario: 泛型 class 的 T 槽（形态 ②，兼修 use-after-free）
- **WHEN** `class CBox<T> { T Item; }`，执行 `P2 c; c.Y = 2; var cb = new CBox<P2>(c); c.Y = 77;`
- **THEN** `cb.Item.Y == 2`；且 `cb.Item` **不再**持有栈帧作用域的 arena 句柄
  —— ctor 帧弹出后再读 `cb.Item` 不是悬垂访问（走 P3b 的堆对象内联字节区）

#### Scenario: 外层复制深拷（形态 ③）
- **WHEN** `var p3 = new Pair<P2,int>(d, 1); var q = p3; q.First.Y = 9;`
- **THEN** `p3.First.Y` 不受影响 —— 外层 `StructCopy` 拷的是内联字节，不是共享句柄

#### Scenario: 嵌套实例化
- **WHEN** `Pair<Pair<P2,int>, int>`
- **THEN** 内外两层都按实例化算布局，内层的 `P2` 字节逐层内联

### Requirement: 闸门 —— 非 blob struct 实参一律不改变表示

型参实参不是 blob struct（引用类型 / 数组 / 接口 / 另一个型参 / prim）时，该字段槽**保持
今天的 8 字节 `GcRef` 表示**，布局与发射**逐字节不变**。

#### Scenario: 引用类型实参
- **WHEN** `Pair<string,int>` / `Pair<object,int>` / `List<P2>` 的 `T[] items` 字段
- **THEN** 布局、`ArrayNewInstr`、`StructAlloc` 的产出与本 change 之前**逐字节相同**

#### Scenario: 全 prim 元组不受影响
- **WHEN** `var t = (1, 2)`（`ValueTuple2<int,int>`，两个字段都是型参）
- **THEN** 不产生按实例化描述符，产出字节不变

### Requirement: 按实例化的类描述符随 TYPE 段投送，零格式 bump

每个**实际被用到**且命中闸门的实例化，在 zbc TYPE 段**追加一条合成类描述符**，
名字为该实例化的规范名，携带其 struct 布局块（并在泛型 class 情形携带内联布局块）。

#### Scenario: reader 兼容
- **WHEN** 旧 reader 读到含合成描述符的 TYPE 段
- **THEN** 正常读完（TYPE = `count:u32` + `count` × 描述符，多几条只是列表变长）
  —— **不 bump zbc / zpkg minor**

#### Scenario: 运行期按名取用
- **WHEN** 运行期对 `Pair<P2,int>` 求布局
- **THEN** `exec_struct.rs::resolve_layout` 以**原样字符串名** `try_lookup_type` 命中该描述符、
  返回其 `struct_layout`；**运行期代码零改动**

### Requirement: 实例化规范名三方一致

编译器、wire、运行期对同一实例化必须拼出**同一个**名字。规范名 = **限定基名 + 原样实参串**，
与数组元素类型名既有约定同源（`ExprEmitter._qualifyElemName`：在 `<` 处切开，只限定基名）。

#### Scenario: 跨包实例化
- **WHEN** 消费方包用自己的 `struct P2` 实例化生产方包的 `struct Pair<A,B>`
- **THEN** 两侧算出的布局**逐字节一致**（沿用 `add-crosspkg-struct-value-semantics` 的
  「消费方重算、与生产方逐字节一致」先例）；规范名中基名按生产方 ns 限定、实参按消费方 ns 限定

## MODIFIED Requirements

### Requirement: `StructLayout._kindOf` 接受代换上下文

**Before:** `_kindOf(typeName)` 只看名字。型参名 `"A"` 既不是 struct 也不是 prim/string
⇒ 落 `StructLeafKind.GcRef`（8 字节句柄）。布局按**定义**算，一个泛型定义一份布局。

**After:** 算实例化布局时带一张型参→实参代换表。`_kindOf("A")` 先经代换表得到 `"P2"`，
再照常判定 ⇒ `StructLeafKind.Struct`。无代换表（算定义布局）时行为**逐字不变**。

### Requirement: 发射端不再把实例化类型塌成定义

**Before:** `FunctionEmitter._blobStructNameT` 对 `Z42InstantiatedType` 取 `.Def`，
用裸名查布局——实例化信息在此丢失。

**After:** 命中闸门的实例化保留实例化规范名；未命中（或非实例化）仍走裸名，**字节不变**。

## 不改变的约束

- **D1-a 不变**：引用叶子仍进侧表，不裸内联进字节区（`struct-value-semantics.md:325-335` 已否决 D1-b）。
- **struct 无 base/vtable** 不变。
- **不做全量单态化**：只具体化**布局**，不为每个实例化生成代码。
- **容器 backing 不在本 change**：`List<T>` 内部 `new T[n]` 仍发字面 `"T"` ⇒ 引用背衬 ⇒
  P3a 装箱继续承重。容器密集化 + 删 P3a 装箱由**紧接的下一条 change** 处理（见 proposal「已裁决」）。

## IR Mapping

- **零格式 bump**：不新增 section、不改既有记录 shape。TYPE 段仅**追加**合成类描述符，
  复用既有 struct 布局块与 `CLASS_FLAG_HAS_INLINE_STRUCT` 内联布局块。
- 无新 opcode：访问复用 `StructFieldGetPrim/SetPrim`（0xC0–0xC3）与 `_copyRegion` / `StructCopy`。

## Pipeline Steps

- [ ] Lexer —（无）
- [ ] Parser —（无）
- [ ] Semantics — `StructLayout` 代换版布局 + 实例化枚举 pass + 发射端保留实例化名
- [ ] IR/Emit — 合成类描述符随 TYPE 段投送
- [ ] Runtime —（预期零改动；`exec_array.rs:33` 的 `split('<')` 可改「先试全名、miss 再剥」，向后兼容）
- [ ] Tests — 见下

## 测试

- golden：三形态各一条（泛型 struct / 泛型 class / 外层复制写穿），**interp + jit 双跑**
- **阴性对照**：撤掉修复、重建编译器，确认三条判红（防「测试写了但抓不到 bug」）
- **闸门回归**：`Pair<string,int>` / 全 prim 元组的产出**逐字节不变**（比对 zbc）
- 既有必须仍绿：`generic_struct_chain.z42`（`:52`/`:62-64`/`:73-75` 的写穿）、
  `struct_generic_container.z42`（P3a 箱/拆箱不受影响）、`tuple_basic.z42`、
  `cross-zpkg/tuple_cross_pkg`、`cross-zpkg/struct_nested_layout_cross_pkg`
- **自举不动点** gen1 == gen2（预留重新供种一轮）
- 跨包：消费方用自己的 struct 实例化生产方泛型 struct，两侧布局逐字节一致
