//! 运行时值与对象模型（hub）—— refactor-split-metadata-types（2026-09-03）：原 2436 行单文件按职责拆到
//! `types/` 子模块；本文件只做汇总，全量 `pub use` 使既有 `crate::metadata::types::X` 路径零改动。
//!
//! | 子模块 | 内容 |
//! |---|---|
//! | `field` | FieldSlot、TAG_* 类型标签、默认值 |
//! | `type_desc` | TypeDesc / TypeDescCold（≈ CoreCLR MethodTable，Arc 共享） |
//! | `layout` | StructTypeLayout / ObjectLayout / InlineRef、compose / synthesize 布局 |
//! | `codec` | inline ref 与基元字节编解码 |
//! | `object` | NativeData / ScriptObject + GcRef<ScriptObject> 访问 |
//! | `array` / `array_access` | ArrayObj / ArrayBacking：构造与 backing 分配 / 元素访问·视图·GC·深拷贝 |
//! | `value` / `value_aux` | Value 枚举 + impl / PartialEq；StructArrayElem、RefKind、Pin、Closure 数据、ExecMode |

mod field;
mod type_desc;
mod layout;
mod codec;
mod object;
mod array;
mod array_access;
mod value;
mod value_aux;

pub use field::*;
pub use type_desc::*;
pub use layout::*;
pub use codec::*;
pub use object::*;
pub use array::*;
pub use array_access::*;
pub use value::*;
pub use value_aux::*;
