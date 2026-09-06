# Tasks: fix-autoprop-getonly-backing-write

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06

**变更说明：** 属性访问不再按**源名**发 `field_get`/`field_set`（那是一个不存在的字段）。四条路径
从「编译干净、运行期读回 `null` / 写入丢失」修正为走访问器或落到后备字段 `__prop_X`；同批引入
**E0452**，把 get-only 属性的可写范围收敛到构造函数内。

**原因：** 属性在符号表以**源名** `X` 登记 `FieldSymbol`（供类型检查），存储却是合成的 `__prop_X`
（`get_X`/`set_X` 读写它）。这条落差从未被发射端消费——又一处 **binder 认、emitter 不认**的不对称，
与 `fix-switch-break-diagnostic`（E0410）同型。四个漏口：

| 路径 | 旧行为 | 根因 |
|------|--------|------|
| `this.X = v`，`X` 无 setter | 按源名 `field_set` → 写进不存在的字段 | setter 派发只在 `ct.Methods` 有 `set_X` 时触发 |
| 类内裸写 `X = v` | 被当成「首次赋值一个新局部」，属性根本没被写 | 裸 ident 路径只认 `_ctx.Fields`，未命中就落局部分配 |
| 类内裸读 `X` | `field_get %0.X` → 恒 `Null` | `_ctx.Fields` 由 `owner.Fields.Keys()`（**源名**）填 |
| 经子类引用读/写基类属性 | `field_get obj.X` → 恒 `Null` | `ct.Methods` **不含继承来的**访问器 |

其中第一条正是 C# 惯用的不可变类写法（`public string Label { get; }` + ctor 内赋值），今天写出来
编译干净、运行期读回 `null`。既有测试 `src/tests/classes/auto_property_class.z42` 用直写内部名
`this.__prop_Label = label` 绕过——注释白纸黑字写着「ctor can still write the backing field」，
是一个被当作用法记录下来的 workaround。

**文档影响：** `docs/book/src/compiler/source-compile.md` 新增机制小节；`DiagnosticCodes.z42` 新增
E0452 条目。

## 1. 根因修复
- [x] 1.1 `Symbol.z42`：`FieldSymbol` 加 `IsProp` / `IsPropNoSetter` / `PropBackingName` 三个标志 + 构造初始化
- [x] 1.2 `MemberCollector.z42`：`PropertyDecl` 分支给 `FieldSymbol` 打标（`PropBackingName` 仅在
      有后备存储时设——static / extern / 计算属性无存储）
- [x] 1.3 `EmitContext.z42`：新增 `InstProps`（名 → 后备名）/ `InstPropHasSet` 两表 + 构造初始化
- [x] 1.4 `FunctionEmitter.z42`：实例方法前导填两表（静态属性不入表，其访问不经 reg0）
- [x] 1.5 `AccessEmitter.z42` 成员路径：getter/setter 判据从「`ct.Methods` 有 `get_X`/`set_X`」放宽为
      「有访问器**或**该成员是属性」——覆盖继承（`_passInheritFields` 把基类 `FieldSymbol` **对象本身**
      并进子类 `Fields`，标志随之带过来；`get_X`/`set_X` 运行期经 vtable 找得到）
- [x] 1.6 `AccessEmitter.z42` 成员写无 setter 时落 `field_set obj.__prop_X`（新增 `_propBackingName`）
- [x] 1.7 `AccessEmitter.z42` 裸 ident 读/写：新增属性分支，**先于** `_ctx.Fields` 判定，
      让裸 `X` 与 `this.X` 同义（读 `vcall get_X`；写 `vcall set_X`，无 setter 落 `__prop_X`）。
      局部遮蔽不受影响——`Locals` 查找本就在更前面。**struct 属主保守不启用**（`OwnerStructName == ""`
      才生效），其 blob 布局另有 `StructFieldGet/SetPrim` 翻转路径，无复现不动
- [x] 1.8 `AssignTyper.z42`：新增 **E0452** ——get-only auto-property 仅 ctor 内经 `this` 可写
      （对标 C# CS0200），计算属性任何位置不可赋值。**这一条与 1.6/1.7 必须同批**：只修存储不加
      判据，会把 `{ get; }` 从「处处可写但写丢」变成「处处可写且真的写进去」，洞更大
- [x] 1.9 `DiagnosticCodes.z42`：登记 E0452 条目（**语义层用字面量 `"E0452"` 发码**，不引用常量——
      同 E0449/E0450/E0451 手法，避 core→semantics 新跨成员符号撞 F2 冷启动 stale-cache）

## 2. 测试
- [x] 2.1 `src/tests/classes/auto_property_access.z42`（e2e，新增）：四条路径 + 继承——
      ctor 内 `this.` 写 get-only / ctor 内裸写有 setter 属性 / 裸读 / 裸写走 setter /
      外部 setter 派发不回归 / 经子类引用读基类属性 / 子类方法内 `this.` 与裸名读基类属性
- [x] 2.2 `src/compiler/z42c.semantics/tests/typecheck/property_access_tests.z42`（单测，新增 9 例）：
      E0452 的正负边界 + 「普通字段 / `readonly` 字段（E0415）不受影响」的不回归断言
- [x] 2.3 `src/tests/classes/auto_property_class.z42`：`this.__prop_Label = label`
      → `this.Label = label`（workaround 归位为正常写法）

## 3. 文档同步
- [x] 3.1 `docs/book/src/compiler/source-compile.md`：类型检查节新增「属性的『源名 ↔ 后备字段名』
      落差（binder ↔ emitter 对称）」小节——四个漏口表 + 三处修复点 + 「为什么 E0452 必须同批」

## 4. 验证
- [x] 4.1 `xtask build compiler` 自建全绿（z42c 自身源码不违反新增的 E0452 判据）
- [x] 4.2 完整 `xtask test` GREEN（改动前一轮）：全 stage ✔，**self-host 不动点 gen1==gen2 3/3**
- [x] 4.3 三个属性 e2e 用例逐一编译 + 运行通过（含既有 `auto_property.z42` 不回归）
- [x] 4.4 完整 `xtask test` GREEN（含新增测试）
- [x] 4.5 分支基于 origin/main 顶 → PR

## 备注
- **自举安全**：`xtask test compiler` 的 **self-host 不动点 gen1==gen2 3/3** 说明改动对 z42c 自身
  字节稳定。原因：z42c 源码里**零处**裸名属性访问（启发式扫描 `src/compiler` = 0 处）。
  无 zbc/zpkg 格式 bump、无新语法 → 不触发 bootstrap-seed 的两-nightly 纪律。
  新增 E0452 是**收紧**判据，若 stdlib/toolchain 有违反会在 GREEN 暴露——实测零处。
- **影响面**：启发式扫描疑似裸用属性名——`src/compiler` 0 / `src/toolchain` 2 / `src/libraries` ~41 /
  `src/tests` 5 / `examples` 4。GREEN 全绿说明这些位置要么是扫描误报，要么其行为本就依赖修复后的语义。
- **本修复是 [[restore-emit-zbc-diagnostics-program]] 的第 4 步第 1 项**（口令「推进诊断可见性」）：
  该程序查明 13 个真编译器 bug，User 裁决「先杀三条静默错误代码」，本条是其一。
  另两条：泛型形参上的运算符发裸 `add`（`where T:INumber<T>` 从未生效）、func 约束形参被 lambda
  捕获时发 `call @f`。
- **范围经 User 裁决扩大**：最初只诊断出「get-only 写丢」一条，实施中发现裸名读/写与继承路径同源，
  User 裁决扩到根因一次修完，而非拆成两次落地。
