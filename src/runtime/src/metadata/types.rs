//! 运行时值与对象模型（hub）—— refactor-split-metadata-types（2026-09-03）：原 2436 行单文件按职责拆到
//! `types/` 子模块；本文件只做汇总，全量 `pub use` 使既有 `crate::metadata::types::X` 路径零改动。
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | `field` | FieldSlot、TAG_* 类型标签、默认值 |
//! | `type_desc` | TypeDesc / TypeDescCold（≈ CoreCLR MethodTable，Arc 共享） |
//! | `layout` | StructTypeLayout / ObjectLayout / FieldAccess、compose / synthesize 布局（对象字段的单元归属） |
//! | `codec` | 基元字节 / 位编解码 |
//! | `ref_word` | 对象引用字段的 8 B 自描述字（种类 + 句柄）与 `Value` 互转 |
//! | `tparam_cell` | 型参字段的 16 B 单元（自描述标签字 + 基元负载字）的读写与 GC 遍历 |
//! | `object` / `object_fields` | NativeData / ScriptObject + GcRef<ScriptObject> 访问；实例字段单元的读写、GC 遍历 |
//! | `array` / `array_access` | ArrayObj / ArrayBacking：构造与 backing 分配 / 元素访问·视图·GC·深拷贝 |
//! | `array_sort` | 基元元素前缀的原生稳定排序（`List<T>.Sort()` 快路径） |
//! | `elem_type` | ElemType：数组元素类型名的进程级驻留句柄（8 B，建数组零分配） |
//! | `value` / `value_aux` | Value 枚举 + impl / PartialEq；StructArrayElem、RefKind、Pin、Closure 数据、ExecMode |

mod field;
mod type_desc;
mod layout;
mod codec;
pub mod ref_word;
mod obj_storage;
mod object;
mod object_fields;
pub(crate) mod tparam_cell;
mod array;
mod array_access;
mod array_sort;
mod elem_type;
mod value;
mod value_aux;

pub use field::*;
pub use type_desc::*;
pub use layout::*;
pub use codec::*;
pub use obj_storage::*;
pub use object::*;
pub use object_fields::FieldWrite;
pub use array::*;
pub use array_access::*;
pub use elem_type::*;
pub use value::*;
pub use value_aux::*;
