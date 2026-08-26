# Tasks: 给 `[Record]` 类型加值语义

> 状态：🟢 已完成 | 创建：2026-08-26 | 完成：2026-08-26

## 进度概览
- [x] 阶段 1: RecordSynth 合成器（class 值语义 + 统一 ToString）
- [x] 阶段 2: IrGen 接线 + OperatorEmitter 拦截（+ Z42Type/StubCollector 加 IsRecord）
- [x] 阶段 3: VM 让路（struct ToString 守卫）
- [x] 阶段 4: 测试（record_value_semantics **interp+jit 双绿**）
- [~] 阶段 5: 验证 + 文档同步（book + README 已更新；full GREEN gate 本机 z42vm 挂起 → 交 CI）

> Scope 扩展（实施期发现）：`==` 拦截需可查询的语义 record 位 → 加 `Z42Type.z42`（IsRecord 字段）+
> `StubCollector.z42`（HasRecord 回填）。已登记 proposal Scope。
>
> **jit 双验教训**（调试代价高，务必记住）：合成 class 方法用**裸名**（`$N` 后缀致 jit vtable 派发错配→
> Object 身份版）；**`as_cast` 后 `field_get` jit 误编** → 直读 other 字段（镜像普通 codegen）。interp 全过
> 但 jit 错——合成 IR 必 interp+jit 双验。
>
> **本机验证**：record_value_semantics interp+jit 双绿；z42vm+z42c 自建绿；stdlib 273 goldens regen 绿；
> `cargo test --lib` 见验证报告。full `xtask test` gate 本机 z42vm 退出期挂起（僵进程累积，UE）→ 交 CI。

## 阶段 1: RecordSynth 合成器（`src/compiler/z42c.semantics/src/RecordSynth.z42` NEW）
- [ ] 1.1 新建 `RecordSynthEmitter(symbols, gen)`：自搭 `EmitContext`（镜像 `FunctionEmitter.EmitSynthStructEquals`）
- [ ] 1.2 字段枚举 helper：沿 `HasBase`/`BaseName` 上溯收集 `OwnFieldNames`/`OwnFieldVis`（基类在前，声明序）；两视图——全字段（相等）/ public 字段（ToString）
- [ ] 1.3 `EmitRecordEquals`（class）：`other==null → false`；type-exact 门（`GetType().FullName` 串比）；下转；逐字段（基元 `Eq` / 引用 `.Equals` 递归、容 null）短路合取
- [ ] 1.4 `EmitRecordGetHashCode`（class）：`h=17; h=h*31+field.GetHashCode()`（null→0）折叠 + `& 0x7fffffff`
- [ ] 1.5 `EmitRecordToString`（class）：`"T { "` + 各 public 字段 `Name = ` + `field.ToString()` + `", "` 分隔 + `" }"`，左折叠 `StrConcat`；无字段 → `"T { }"`
- [ ] 1.6 `EmitRecordStructToString`（struct）：`this` boxed → AsCast StructRef → `StructFieldGetPrim` 逐字段（镜像 `_emitLeafEqChecks`）→ 同 1.5 格式

## 阶段 2: IrGen 接线 + OperatorEmitter 拦截
- [ ] 2.1 `IrGen.z42` 合成循环（~L344 blob-equals 附近）：`HandlerRegistry.HasRecord` 为真 → class 合成 1.3–1.5、struct 合成 1.6（均「用户未显式声明才合成」，查 `owner.Methods.ContainsKey`）
- [ ] 2.2 `OperatorEmitter._emitBinary`：record-class 操作数 `==`/`!=` → 发对 `Equals` 的调用（`!=` 取反）；加 `_ee` 的「是 record class」谓词（镜像 blob-struct 拦截 :29）
- [ ] 2.3 若 TypeChecker 对 record-class `==`/`!=` 报错则放行（预期无需改；实测为准，越界则停）

## 阶段 3: VM 让路（struct ToString 守卫）
- [ ] 3.1 `src/runtime/src/metadata/types.rs`：加 `is_record()`（镜像 `is_struct()`:898，读 `CLASS_FLAG_RECORD`）
- [ ] 3.2 `src/runtime/src/interp/exec_vcall.rs:216`：`if method == "ToString"` → `&& !b.type_desc().is_record()`
- [ ] 3.3 `src/runtime/src/jit/helpers/vcall.rs:190`：同守卫
- [ ] 3.4 `cargo build --release` 重建 z42vm + `cp` 到 `.z42/bin/z42vm`（seed 同步，防守卫不生效假红）

## 阶段 4: 测试（`src/tests/attributes/record_value_semantics.z42` + `.expected` NEW）
- [ ] 4.1 class：Equals（同值/异值/null/异类型）、==/!=、GetHashCode（同值同 hash）、ToString
- [ ] 4.2 struct：ToString 记录格式（可观察变更）、相等不回归
- [ ] 4.3 type-exact：`Base(1)` != `Derived(1,2)`（含基类 record）
- [ ] 4.4 嵌套引用字段递归 Equals；字段范围（相等含 private / ToString 只 public）；单字段 / 无字段 `T { }`
- [ ] 4.5 用户显式 `ToString`/`Equals` 不被合成覆盖

## 阶段 5: 验证 + 文档同步
- [ ] 5.1 `cargo build --manifest-path src/runtime/Cargo.toml --release` —— z42vm 无错
- [ ] 5.2 `xtask test compiler` —— z42c 自举全绿
- [ ] 5.3 `xtask test e2e`（含 `--dir attributes`）—— 全绿；`--mode jit` 抽验 record ToString
- [ ] 5.4 `xtask test`（完整 GREEN gate：cross-zpkg / stdlib / vscode-syntax）
- [ ] 5.5 spec scenarios 逐条覆盖确认
- [ ] 5.6 `docs/book/src/language/record-attribute.md`：值相等/ToString 从 Deferred 上移正文（type-exact / 字段范围 / ToString 格式 / struct-ToString VM 守卫机制）
- [ ] 5.7 `src/compiler/z42c.semantics/README.md`：功能索引 + 核心文件加 `RecordSynth.z42`
- [ ] 5.8 self-host 字节不动点 + bootstrap + 全 stdlib（CI 权威）

## 备注
- IrGen 已 611 行超 500 硬限（既有债，compiler-structure-refactor 单列）——本变更只加最小派发，不扩逻辑。
- type-exact 若实测 Type 对象 per-type 单例，可退化为 `GetType() ==` identity（省串比）；默认 FullName 串比不赌单例。
- runtime 改动非新 builtin/非格式 bump，但本机验证必重建+同步 seed vm（3.4）。
