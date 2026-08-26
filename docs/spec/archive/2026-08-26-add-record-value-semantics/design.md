# Design: 给 `[Record]` 类型加值语义

## Architecture

```
[Record] class/struct  ──parse──▶ ClassDecl(+主构造器展开 public 字段) + [Record] attr
                                          │
                              IrGen 合成循环（每类型发完 TYPE + 用户方法后）
                                          │  HandlerRegistry.HasRecord(rawDecl)?
                                          ▼
                                  RecordSynth（新文件）
             ┌──────────────────────────┼─────────────────────────────┐
        [Record] class                                          [Record] struct
        （引用类型，vtable 覆盖）                                  （值类型，boxed vcall）
   ├ Equals$1(object)  member-wise + type-exact              ├ Equals$1  ← 既有 blob 路已合成（不动）
   ├ GetHashCode()     组合各字段 hash                        ├ GetHashCode ← 既有 native FNV（不动，已是值哈希）
   ├ ToString()        "T { A = v, ... }"                    └ ToString()  "T { A = v, ... }"  ← 新合成
   └ op ==/!=  ← OperatorEmitter 拦截 → 调 Equals                  │
                                                        VM 让路：exec_vcall/jit vcall 的零参 ToString
                                                        原生拦截加 `!is_record()` 守卫 → 落到合成方法
```

合成产物全部复用既有 IR 指令（`IsInstance`/`AsCast`/`FieldGet`/`StructFieldGetPrim`/`Eq`/`Call`/
`StrConcat`/`BrCond`/`ConstBool`/算术）。**无新 IR 指令、无 zbc/zpkg 格式 bump。**

## Decisions

### Decision 1: struct 记录式 ToString —— VM 让路 + 编译器合成（S1）

**问题：** VM 对 boxed-struct 零参 `ToString` **无条件原生拦截**返回短类型名（interp `exec_vcall.rs:216-220`、
JIT `jit/helpers/vcall.rs:190-196`），在候选查找之前 return，合成的 `<Type>.ToString` 够不着。给 record
struct 记录格式，必须绕过此拦截。

**选项：**
- **S1（VM 让路 + 编译器合成）**：给 `TypeDesc` 加 `is_record()`；两处原生 ToString 拦截加 `!is_record()`
  守卫 → record struct 落到候选查找 → 命中合成的 `<Type>.ToString`。格式化逻辑全在编译器合成。
  VM 改动 = 1 个访问器 + 2 处一行守卫。
- **S2（VM 原生格式化）**：VM 原生拦截里，若 record bit 置位，读 struct 布局 + 字段名/值递归格式化成
  `T { .. }`。编译器不为 struct ToString 合成。VM 改动大（枚举字段名/值/递归），且与 class 侧
  （编译器合成）格式**两处实现易漂移**。

**决定：选 S1。** 格式化的**单一真相源**留在编译器（class & struct 共用一套 RecordSynth ToString 逻辑，
格式天然一致），VM 只做「record → 别拦截」的最小让路。符合根因修复 / 设计完整性（不在 VM 里重复一份
易漂移的格式化）。

**GetHashCode 不改**：struct 的 native FNV 已是「等值→等哈希」的值哈希；record struct 沿用，守卫只加在
ToString、不加在 GetHashCode。

### Decision 2: type-exact 相等门 —— 比 `GetType().FullName`

**问题：** C# record 用 EqualityContract 做 type-exact（`Base(1) != Derived(1,2)` 即使基字段同）。z42 需
一个可靠的「运行期类型完全相同」判定。`other is T`（`IsInstance`）是 is-a（子类也过），不满足 exact。

**选项：** A `this.GetType() == other.GetType()`（依赖 Type 对象 per-type 单例，identity 比较）；
B `this.GetType().FullName == other.GetType().FullName`（字符串比较，与 Type 对象身份语义无关）。

**决定：选 B（FullName 字符串比较）。** 不依赖「Type 对象是否单例」这一未坐实前提，robust。合成 Equals 骨架：

```
bool Equals(object other) {
    if (other == null) return false;
    if (other.GetType().FullName != this.GetType().FullName) return false;   // type-exact 门
    T o = (T)other;                                                          // 门后安全下转
    return this.f1 == o.f1 && this.f2.Equals(o.f2) && ...;                   // 逐字段（基元 Eq / 引用 .Equals）
}
```

> 若实施期实测 Type 对象确为 per-type 单例，可退化为 A（省一次 FullName 取串+串比）——但默认 B，不赌单例。

### Decision 3: 参与字段的枚举与可见性范围（对齐 C#）

**数据源**：`Z42ClassType.OwnFieldNames` / `OwnFieldVis` / `OwnFieldCount`——**确定性声明序** + 逐字段可见性
（TSIG 有序元数据，非 hashed `Fields`）。含基类时沿 `HasBase`/`BaseName` 上溯，**基类字段在前、派生在后**
（对齐 C# PrintMembers base-first）。

- **相等**：比**全部实例字段**（所有可见性，含 private / 位置字段 / 块内声明）。与既有 struct blob-equals
  「比所有叶子」一致。
- **ToString**：只打 `OwnFieldVis == "public"` 的字段。private 字段参与相等但不出现在 ToString。

字段值比较分派：
- **基元/值字段**（int/bool/char/...）：`Eq` 指令。
- **引用字段**（string/object/record/数组）：调该字段的 `.Equals`（`Call` boxed vcall，递归值比较）。
  null 字段：`Equals` 需容 null（`a == null ? b == null : a.Equals(b)`——或统一走 `Std.Object.Equals` 静态
  helper 若存在；实施时择一，spec 场景「嵌套引用字段」覆盖）。

### Decision 4: 合成落点 —— 新文件 `RecordSynth.z42` + IrGen 最小接线 + OperatorEmitter 拦截

**问题：** 合成逻辑放哪？`FunctionEmitter.z42` 已 418 行、`IrGen.z42` 已 **611 行超 500 硬限**（既有债，
compiler-structure-refactor 程序单列拆分）——不宜再往里堆。

**决定：**
- **新文件 `RecordSynth.z42`**：`RecordSynthEmitter(symbols, gen)`，镜像 `FunctionEmitter.EmitSynthStructEquals`
  自搭 `EmitContext` 发 body。方法：`EmitRecordEquals` / `EmitRecordGetHashCode` / `EmitRecordToString`
  （class）+ `EmitRecordStructToString`（struct，`this` 为 boxed → AsCast StructRef → `StructFieldGetPrim`
  逐字段，镜像 `_emitLeafEqChecks` 的字段访问）。
- **IrGen** 合成循环（发完 struct blob-equals 那段附近，~L344）加**最小派发**：`HandlerRegistry.HasRecord`
  为真时，class → 合成 4 方法（用户未显式声明者才合成），struct → 只合成 ToString。仅加派发调用、逻辑在
  RecordSynth。
- **OperatorEmitter** `_emitBinary`：record-class 两操作数的 `==`/`!=` → 发对 `Equals` 的调用（镜像 blob-struct
  `==` 拦截 OperatorEmitter:29；判定用 `_ee` 提供的「是 record class 类型」谓词）。`!=` = `Equals` 取反。

**「用户未显式声明才合成」**：查 `owner.Methods.ContainsKey("Equals$1")` / `"ToString"` / `"GetHashCode"` /
`"ToString$0"`（镜像既有 struct-equals 的 `!owner.Methods.ContainsKey("Equals$1")` 守卫 IrGen:348）。

### Decision 5: GetHashCode 组合算法（class）

逐字段 hash 折叠（对齐 .NET 惯用式，确定、等值等哈希）：`h = 17; foreach f: h = h * 31 + (f == null ? 0 :
f.GetHashCode())`，末尾 `& 0x7fffffff`（与 native FNV 的正数掩码一致）。基元字段的 `.GetHashCode` 走
boxed-primitive 路（§Explore：基元盒 ToString/GetHashCode 不被 struct native 臂拦截，落 `Std.Int32.*` 等）。
type-exact 无需进 hash（等值必等类型，hash 只需等值等哈希；类型不同碰撞可接受）。

### Decision 6: 无格式 bump、单 PR、自举影响

- **无 zbc/zpkg 格式 bump**：record bit3 早在格式中（`ClassDescBuilder` 已写、`CLASS_FLAG_RECORD` 已定义）。
- **无新语法** → **不受两-nightly support-先行纪律约束**，单 PR。
- **runtime 改动**（`is_record()` + 2 处守卫）非新 builtin、非格式变更；但本机验证需 `cargo build --release`
  重建 z42vm + 同步 seed vm（`.z42/bin/z42vm`），否则 boxed-struct ToString 守卫不生效 → 假红（见
  [[sync-seed-vm-after-rebase-adds-runtime-builtin]] 同类教训）。
- **z42c 自身源码不用 record** → 无自依赖环、无 cold-seed 风险。

## Implementation Notes

- **合成方法命名**：class 覆盖 Object 虚方法用无 arity 名 `<FQType>.ToString` / `<FQType>.Equals$1` /
  `<FQType>.GetHashCode`（vtable 覆盖需与 `Std.Object.*` 同名/同签；Equals 取 `$1` 对齐既有 struct 合成）。
  struct ToString 用 `<FQType>.ToString`（候选查找第二候选；或 `ToString$0` 匹配 `$arity` 首候选——实施时对齐
  `exec_vcall.rs:222-248` 查找序）。
- **class 字段访问**：`FieldGet(reg0=this, fieldName)`；**struct 字段访问**：AsCast boxed→StructRef 后
  `StructFieldGetPrim(off, tag)`（镜像 `_emitLeafEqChecks`）。
- **ToString 串拼接**：左折叠 `StrConcat`（`ExprEmitter` 已有 `EmitConcat` 范式，L274）。字段值先各自
  `.ToString()`（`Call` boxed vcall）。定长前后缀 `"T { "` / `" }"` 与分隔 `", "` / `" = "` 用 `ConstStr`。
- **VM 守卫**：`types.rs` 加 `#[inline] pub fn is_record(&self) -> bool { self.class_flags & CLASS_FLAG_RECORD != 0 }`；
  `exec_vcall.rs` 与 `jit/helpers/vcall.rs` 的 `if method == "ToString"` 前置改 `if method == "ToString" && !b.type_desc().is_record()`。

## Testing Strategy

- **Golden e2e**（`src/tests/attributes/record_value_semantics.z42` + `.expected`）：逐条覆盖 spec 场景——
  class Equals/==/!=/GetHashCode/ToString、struct ToString（可观察变更）、type-exact（Base≠Derived）、null、
  异类型、嵌套引用字段递归、public/private 字段范围（相等含 private、ToString 不含）、单字段、无字段
  `T { }`、用户显式 ToString 不被覆盖。
- **VM 验证**：`xtask test`（完整 GREEN gate）——interp e2e + cross-zpkg + stdlib + compiler 自举 +
  vscode-syntax。JIT 守卫由 CI `test-vm-jit` 覆盖（本机可 `test e2e --mode jit` 抽验 record ToString）。
- **自举字节不动点 + 全 stdlib + bootstrap** 交 CI。
