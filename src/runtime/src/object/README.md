# object/

## 职责

定义值与托管对象的内存表示、布局计算和存储原语。包内静态声明由 metadata 描述，类型注册、身份分配和派发缓存由 VM 链接状态持有。

## 功能索引

- `value/`：Value、原子单元、引用字、型参单元和 struct blob。
- `types/`：TypeDesc 形状、字段、对象/struct 布局、元素类型及泛型实参句柄。
- `header/`：区块头、标记位、强/弱句柄与 generation 快照。
- `object/`：对象与堆外对象存储、字段访问和引用遍历。
- `array/`：数组描述符、元素、视图及排序原语。
- `string/`：不可变字符串句柄与字符索引。

## 基础用法

公共值词汇可从 `crate::object::{Value, TypeDesc, ScriptObject, ArrayObj, Str, ObjRef, ArrRef}` 导入；区块布局从 `z42::object::header` 导入。
GC 仍负责 chunk 分配、回收、代数及弱句柄的 chunk epoch 校验；需要堆或运行期服务的既有调用由分层基线持续约束。

## 如何测试验证

在仓库根运行 `./xtask test runtime`。单跑对象用例可用：

```bash
(cd src/runtime && cargo test object:: -- --test-threads=1)
```

## 关联文档

内存契约见 [对象与值的表示 ABI](../../../../docs/internals/src/runtime/object-abi.md)，分配与句柄存活协议见 [GC 子系统](../../../../docs/internals/src/runtime/gc.md)。

## 核心文件

`mod.rs` 汇总公共词汇；各 concern 的 `mod.rs` 汇总对应叶子实现。原有 ABI 布局、对象访问、引用字、数组及字符串测试与实现同目录。
