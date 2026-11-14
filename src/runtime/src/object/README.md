# object/

## 职责

定义值与托管对象的内存表示、布局计算和存储原语。包内静态声明由 z42-zpkg 描述，类型注册、身份分配和派发缓存由 VM 链接状态持有。

## 功能索引

- `value/`：Value、`payloads.rs` 中的引用/pin/闭包载荷、原子单元、引用字、型参单元、struct blob、纯值种类名称，以及 context-free 格式化和盒装值哈希。
- `types/`：TypeDesc 形状、字段、对象/struct 布局、元素类型及泛型实参句柄。
- `protocol.rs`：GetType、默认文本和 Type 构造的只读能力契约；上下文和错误域由调用者指定，不依赖执行层实现。
- `header/`：区块头、标记位、强/弱句柄与 generation 快照。
- `object/`：对象与堆外对象存储、字段访问、引用遍历，以及 native Monitor 的同步状态。
- `array/`：数组描述符、元素、视图及排序/检索原语。
- `string/`：不可变字符串句柄、字符索引与 ASCII 集合字节扫描（长串 SSE2 / 标量回落）。

## 基础用法

对象能力契约通过 `object::ObjectProtocol` 消费；`protocol.rs` 为私有实现，调用方不引用其文件路径。

公共值词汇可从 `crate::object::{Value, TypeDesc, ScriptObject, ArrayObj, Str, ObjRef, ArrRef}` 导入；区块布局从 `z42::object::header` 导入。
格式化与哈希的 crate 内部实现位于 `value/format.rs` 与 `value/boxed_hash.rs`，由执行层和 builtin 共同消费。
GC 仍负责 chunk 分配、回收、代数及弱句柄的 chunk epoch 校验；需要堆或运行期服务的既有调用由分层基线持续约束。
协议实现由 corelib 提供并在 VM 装配时注入 Core；worker 共享所属 Core 的表。托管 ToString 调用、异常处理和默认文本的 GC 物化仍归 exec，具体机制见 [VM 总体架构](../../../../docs/internals/src/runtime/vm-architecture.md)。

## 如何测试验证

在仓库根运行 `./xtask test runtime`。单跑对象用例可用：

```bash
(cd src/runtime && cargo test object:: -- --test-threads=1)
```

命名 struct 字段的布局查询在 `types/named_struct_layout.rs`，按类型描述重建字段偏移并核对已交付的布局。

## 关联文档

内存契约见 [对象与值的表示 ABI](../../../../docs/internals/src/runtime/object-abi.md)，分配与句柄存活协议见 [GC 子系统](../../../../docs/internals/src/runtime/gc.md)。

## 核心文件

`mod.rs` 汇总公共词汇；各 concern 的 `mod.rs` 汇总对应叶子实现。原有 ABI 布局、对象访问、引用字、数组及字符串测试与实现同目录。
