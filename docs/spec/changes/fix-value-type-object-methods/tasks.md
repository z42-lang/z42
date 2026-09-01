# Tasks: 值类型 + Type 对象 Object 方法 / 数组全路径名

> 状态：🟡 进行中 | 创建：2026-09-01 | 类型：vm/lang（编译器派发 + 反射）

## 进度概览
- [ ] 阶段 0: ③ Type 对象 GetType null 根因定位
- [ ] 阶段 1: struct/enum GetType 折叠 typeof
- [ ] 阶段 2: struct ToString/Equals/GetHashCode 装箱 + VCall
- [ ] 阶段 3: ③ 修复 + ④ 数组全路径名
- [ ] 阶段 4: 测试
- [ ] 阶段 5: 文档同步
- [ ] 阶段 6: 验证 GREEN

## 阶段 0: ③ 根因定位（先钉准再改）
- [ ] 0.1 加诊断/探针定位 `typeof(X).GetType()`（Std.Type receiver）为何返 null——走 VCall？折叠？DepIndex？
- [ ] 0.2 确定修复点（SymbolCollector Object-stub GetType 派发 vs CallEmitter），记入 design 决策 4

## 阶段 1: struct/enum GetType 折叠 typeof
- [ ] 1.1 CallEmitter：识别值类型（struct/enum）receiver + `GetType` 0 参 → 发 `Typeof(静态类型FQN)`
- [ ] 1.2 验证 struct A / enum E 的 GetType（FullName/IsValueType/IsEnum）

## 阶段 2: struct ToString/Equals/GetHashCode 装箱 + VCall
- [ ] 2.1 CallEmitter struct 路径：method∈{ToString,Equals,GetHashCode} 且 struct 未自声明 → `__box_struct` + VCall
- [ ] 2.2 record / 用户自声明方法仍走自身（不被装箱协议改写）——回归确认
- [ ] 2.3 CallEmitter/ClassExtractor：订正「ExcludeFromImplicitObject 误解」注释，指向 design 决策 3

## 阶段 3: ③ 修复 + ④ 数组全路径
- [ ] 3.1 按阶段 0 结论根因修 Type 对象 GetType → typeof(Std.Type)
- [ ] 3.2 runtime `type_object.rs` build_type_ex：数组 FullName=`{elemFullName}[]`、Name=`{elemName}[]`（递归）

## 阶段 4: 测试
- [ ] 4.1 `src/tests/types/value_type_object_methods.z42`：struct 四方法 + enum GetType + Type 对象 GetType + class 基线
- [ ] 4.2 `src/tests/types/array_type_fullname.z42`：int[]/int[][]/用户类数组 FullName+Name；GetElementType 不变
- [ ] 4.3 `reflection_tests.rs`：数组全路径名单测
- [ ] 4.4 回归：record ToString/Equals 用例仍绿

## 阶段 5: 文档同步
- [ ] 5.1 `docs/design/language/reflection.md`：值类型 Object 方法派发 + 数组全路径名机制
- [ ] 5.2 `docs/book/src/runtime/struct-value-semantics.md`：struct 的 Object 方法（装箱派发）如相关
- [ ] 5.3 CallEmitter/ClassExtractor 注释订正落地

## 阶段 6: 验证 GREEN
- [ ] 6.1 `cargo build --release` + `cargo test --lib`
- [ ] 6.2 `xtask test e2e --dir types` + record/enum 相关 dir
- [ ] 6.3 `xtask test` 完整 gate（含 z42c 自举字节不动点——确认编译器改动不破自举）
- [ ] 6.4 spec scenarios 逐条覆盖

## 备注
- 决策 3 若实施中被迫转选项 B（解除 struct 排除，改 zbc 元数据）→ 停下问 User（格式/自举字节影响）。
- enum 的 ToString/Equals/GetHashCode 不在本次（Out of Scope）。
