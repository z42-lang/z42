//! `OpError`：objops 唯一的错误通道。两个引擎各自只做一次映射——
//! interp 把它变成 `Ok(Some(exc))`（值抛出）或 `Err`（内部错误），
//! JIT helper 把它塞进 pending 异常槽并返回 1。
//!
//! 用户可见的异常类与消息文本**只在本文件**定义；任何一侧都不得自己拼一条。

use crate::metadata::{Module, Value};
use crate::vm_context::VmContext;

pub const NULL_REF_EXC: &str = crate::semantics::NULL_REF_EXC;
pub const INVALID_CAST_EXC: &str = crate::semantics::INVALID_CAST_EXC;
pub const INDEX_OOR_EXC: &str = "Std.IndexOutOfRangeException";
pub const OVERFLOW_EXC: &str = "Std.OverflowException";
pub const OOM_EXC: &str = "Std.OutOfMemoryException";

pub type OpResult<T> = Result<T, OpError>;

/// 可抛给用户代码的异常：stdlib 异常类 + 消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Throw {
    pub class: &'static str,
    pub msg: String,
}

#[derive(Debug)]
pub enum OpError {
    /// 用户可 `catch` 的异常，由引擎按类名构造。
    Throw(Box<Throw>),
    /// 已经构造好的异常对象（类型初始化失败、缺符号）。
    Thrown(Value),
    /// VM 内部错误：编译器发错码、句柄失效、布局损坏。不是用户能触发的语义。
    Internal(anyhow::Error),
}

/// 数组操作种类，只用于消息文本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrayOp { Get, Set, Len, Addr }

impl ArrayOp {
    fn verb(self) -> &'static str {
        match self {
            ArrayOp::Get  => "read an element of",
            ArrayOp::Set  => "write an element of",
            ArrayOp::Len  => "read the length of",
            ArrayOp::Addr => "take an element address of",
        }
    }
    pub fn opcode(self) -> &'static str {
        match self {
            ArrayOp::Get  => "ArrayGet",
            ArrayOp::Set  => "ArraySet",
            ArrayOp::Len  => "ArrayLen",
            ArrayOp::Addr => "LoadElemAddr",
        }
    }
}

impl OpError {
    #[cold]
    pub fn throw(class: &'static str, msg: String) -> Self {
        OpError::Throw(Box::new(Throw { class, msg }))
    }

    #[cold]
    pub fn internal(msg: String) -> Self {
        OpError::Internal(anyhow::anyhow!(msg))
    }

    /// 读字段时接收者为 null。
    #[cold]
    pub fn null_field_read(field: &str) -> Self {
        Self::throw(NULL_REF_EXC, format!("cannot read field `{field}` of a null reference"))
    }

    /// 写字段时接收者为 null。
    #[cold]
    pub fn null_field_write(field: &str) -> Self {
        Self::throw(NULL_REF_EXC, format!("cannot write field `{field}` of a null reference"))
    }

    /// 取字段地址（`ref obj.f`）时接收者为 null。
    #[cold]
    pub fn null_field_addr(field: &str) -> Self {
        Self::throw(NULL_REF_EXC, format!("cannot take the address of field `{field}` of a null reference"))
    }

    /// 数组操作的数组为 null。
    #[cold]
    pub fn null_array(op: ArrayOp) -> Self {
        Self::throw(NULL_REF_EXC, format!("cannot {} a null array", op.verb()))
    }

    /// 下标越界（含负数）。
    #[cold]
    pub fn index_out_of_range(index: i64, len: usize) -> Self {
        Self::throw(INDEX_OOR_EXC,
            format!("index {index} is out of range for an array of length {len}"))
    }

    /// 数组长度为负。
    #[cold]
    pub fn negative_array_size(n: i64) -> Self {
        Self::throw(OVERFLOW_EXC, format!("array size cannot be negative (got {n})"))
    }

    /// 基元字段拒收的值：null → `NullReferenceException`（与「null 拆箱到值类型」同族），
    /// 其余 → `InvalidCastException`。
    #[cold]
    pub fn field_store_rejected(field: &str, v: &Value) -> Self {
        if matches!(v, Value::Null) {
            Self::throw(NULL_REF_EXC, format!("cannot store null into primitive field `{field}`"))
        } else {
            Self::throw(INVALID_CAST_EXC, format!(
                "cannot store {} into primitive field `{field}`",
                crate::semantics::value_kind_name(v)))
        }
    }

    /// 分配失败（严格 OOM 模式下堆满）。
    #[cold]
    pub fn oom(what: String) -> Self {
        Self::throw(OOM_EXC, format!("cannot allocate {what}: heap limit exceeded"))
    }

    /// 物化成引擎要抛出的异常值。`Err` = 内部错误（interp 直接返回，JIT 退化成字符串异常）。
    ///
    /// stdlib 异常类没加载（裸模块的 Rust 单测）时同样落到 `Err`，文本为 `<类名>: <消息>`，
    /// 两个引擎拿到的是同一条。
    #[cold]
    pub fn into_exception(self, ctx: &VmContext, module: Option<&Module>) -> anyhow::Result<Value> {
        match self {
            OpError::Thrown(v) => Ok(v),
            OpError::Internal(e) => Err(e),
            OpError::Throw(t) => {
                let built = match module {
                    Some(m) if t.class == OOM_EXC => {
                        // make_oom_exception 暂时关掉严格 OOM 再分配异常对象。
                        let v = crate::exception::make_oom_exception(ctx, m, t.msg.clone());
                        (!matches!(v, Value::Null)).then_some(v)
                    }
                    Some(m) => crate::exception::make_stdlib_exception(ctx, m, t.class, t.msg.clone()).ok(),
                    None => None,
                };
                built.ok_or_else(|| anyhow::anyhow!("{}: {}", t.class, t.msg))
            }
        }
    }

    /// 只能报内部错误的调用点（ref 解引用写回等）用：异常折成同一条 `<类名>: <消息>` 文本。
    #[cold]
    pub fn into_anyhow(self) -> anyhow::Error {
        match self {
            OpError::Internal(e) => e,
            OpError::Throw(t) => anyhow::anyhow!("{}: {}", t.class, t.msg),
            OpError::Thrown(v) => anyhow::anyhow!("exception: {v:?}"),
        }
    }
}

impl From<anyhow::Error> for OpError {
    #[cold]
    fn from(e: anyhow::Error) -> Self { OpError::Internal(e) }
}
