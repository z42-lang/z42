//! `BUILTINS` —— builtin ABI v2 的类型化 builtin 表:`#[z42_macros::builtins]` 模块各自的
//! `DESCS` 编译期拼接(`abi::concat_descs`)。
//!
//! 下标就是 `BuiltinId`。zbc 里存的是**名字**（`BuiltinInsn { dst, name, args }`），`BuiltinId` 由
//! resolver 在**加载期**按名字填进 `Function.resolved.builtin_tokens`，AOT 也不烤它 ⇒ 它是单次运行内的
//! 派发令牌，不跨进程持久化。
//!
//! **一个模块一行**——模块里写了 `#[builtin]` 即登记，不再逐函数列表。加一个 builtin 的三步见
//! `corelib/README.md`「加一个 builtin」。

use super::abi;

const TYPED: &[&[abi::BuiltinDesc]] = &[
    super::convert::typed::DESCS,
    super::math::typed::DESCS,
    super::object::typed::DESCS,
    super::platform::typed::DESCS,
    super::system::typed::DESCS,
    super::config::typed::DESCS,
    super::appprops::typed::DESCS,
    super::symavail::typed::DESCS,
    super::bench::typed::DESCS,
    super::crypto::typed::DESCS,
    super::gc::typed::DESCS,
    super::char::typed::DESCS,
    super::string::typed::DESCS,
    super::reflection::enums::typed::DESCS,
    super::reflection::type_query::typed::DESCS,
    super::reflection::fields::typed::DESCS,
    super::reflection::methods::typed::DESCS,
    super::reflection::properties::typed::DESCS,
    super::reflection::attributes::typed::DESCS,
    super::reflection::generics::typed::DESCS,
    super::reflection::methodof::typed::DESCS,
    super::reflection::invoke::typed::DESCS,
    super::reflection::accessors::typed::DESCS,
    super::reflection::module_load::typed::DESCS,
    super::assemblyloadcontext::typed::DESCS,
    super::threading::typed::DESCS,
    super::fs::typed::DESCS,
    super::fs_backend::memory::typed::DESCS,
    super::process::typed::DESCS,
    super::io::typed::DESCS,
    super::repl::typed::DESCS,
    super::repl_editing::typed::DESCS,
    #[cfg(feature = "native-interop")]
    crate::native::ext::typed::DESCS,
    super::monitor::typed::DESCS,
    #[cfg(feature = "net")]
    super::network::tcp::typed::DESCS,
    #[cfg(feature = "net")]
    super::network::udp::typed::DESCS,
    #[cfg(not(feature = "net"))]
    super::network::unsupported::net::DESCS,
    #[cfg(feature = "tls")]
    super::tls::typed::DESCS,
    #[cfg(not(feature = "tls"))]
    super::network::unsupported::tls::DESCS,
    super::diagnostics::typed::DESCS,
    super::array::typed::DESCS,
    #[cfg(test)]
    super::abi::test_builtins::DESCS,
];

const JOINED: [abi::BuiltinDesc; super::abi::descs_len(TYPED)] = super::abi::concat_descs(TYPED);

pub(crate) const BUILTINS: &[abi::BuiltinDesc] = &JOINED;
