//! 运行时值与对象模型（hub）—— refactor-split-metadata-types（2026-09-03）：原 2436 行单文件按职责拆到
//! `types/` 子模块；本文件只做汇总，全量 `pub use` 使既有 `crate::metadata::types::X` 路径零改动。
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | `field` | FieldSlot、TAG_* 类型标签、默认值 |
//! | `type_desc` | TypeDesc / TypeDescCold（≈ CoreCLR MethodTable，Arc 共享） |
//! | `layout` | StructTypeLayout / ObjectLayout / FieldAccess、compose / synthesize 布局（对象字段的单元归属） |
//! | `codec` | 基元字节 / 位编解码 |
//! | `cells` | 堆上单元的同宽原子访问视图（`Cells`：读写都是 `&self`），对象与数组块共用 |
//! | `ref_word` | 8 B 自描述引用字（种类 + 句柄）与 `Value` 互转；引用单元的读写 / 遍历 |
//! | `tparam_cell` | 16 B 型参单元（自描述标签字 + 基元负载字）的读写与 GC 遍历：型参字段、合成布局的引用字段、MIXED 数组元素 |
//! | `struct_blob` | 堆上的值 struct blob（内联字段 / 装箱 / `struct[]` 元素）：引用叶子是引用字；与帧 arena 形态互转 |
//! | `object` / `object_fields` | NativeData / ScriptObject + crate::gc::ObjRef 访问；实例字段单元的读写、GC 遍历 |
//! | `array` / `array_elems` / `array_access` | ArrayObj / ArrayDesc：一个变长区块（头 + 元素）；模式（REF / MIXED / PRIM / STRUCT / FROZEN）与构造 / 元素读写 / 视图·GC·拷贝·深拷贝 |
//! | `array_sort` | 基元元素前缀的原生稳定排序（`List<T>.Sort()` 快路径） |
//! | `elem_type` | ElemType：数组元素类型名的进程级驻留句柄（8 B，建数组零分配） |
//! | `value` / `value_aux` | Value 枚举 + impl / PartialEq；StructArrayElem、RefKind、Pin、Closure 数据、ExecMode |

mod field;
mod type_desc;
mod layout;
mod codec;
mod cells;
pub mod ref_word;
pub(crate) mod struct_blob;
mod owned_object;
mod object;
mod object_fields;
pub(crate) mod tparam_cell;
mod array;
mod array_elems;
mod array_access;
mod array_sort;
mod elem_type;
mod type_args;
mod value;
mod value_aux;

pub use field::*;
pub use type_desc::*;
pub use layout::*;
pub use codec::*;
pub use cells::Cells;
pub use owned_object::OwnedObject;
pub use object::*;
pub use object_fields::FieldWrite;
pub use array::*;
pub use elem_type::*;
pub use type_args::{TypeArgs, TypeArgsInfo};
pub use value::*;
pub use value_aux::*;
