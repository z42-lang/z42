//! `BUILTINS` —— builtin ABI v2 的类型化 builtin 表:`#[z42_macros::builtins]` 模块各自的
//! `DESCS` 编译期拼接(`abi::concat_descs`)。
//!
//! 下标就是 `BuiltinId`。zbc 里存的是**名字**（`BuiltinInsn { dst, name, args }`），`BuiltinId` 由
//! resolver 在**加载期**按名字填进 `Function.resolved.builtin_tokens`，AOT 也不烤它 ⇒ 它是单次运行内的
//! 派发令牌，不跨进程持久化。
//!
//! **一个模块一行**——模块里写了 `#[builtin]` 即登记，不再逐函数列表。加一个 builtin 的三步见
//! `src/runtime/src/corelib/README.md`「加一个 builtin」。

use crate::vm::exec::call::builtin_abi as abi;

const TYPED: &[&[abi::BuiltinDesc]] = &[
    crate::corelib::convert::typed::DESCS,
    crate::corelib::math::typed::DESCS,
    crate::corelib::object::typed::DESCS,
    crate::corelib::os::platform::typed::DESCS,
    crate::corelib::os::system::typed::DESCS,
    crate::corelib::config::typed::DESCS,
    crate::corelib::appprops::typed::DESCS,
    crate::corelib::symavail::typed::DESCS,
    crate::corelib::bench::typed::DESCS,
    crate::corelib::crypto::typed::DESCS,
    crate::corelib::gc::typed::DESCS,
    crate::corelib::char::typed::DESCS,
    crate::corelib::string::typed::DESCS,
    crate::corelib::reflection::enums::typed::DESCS,
    crate::corelib::reflection::type_query::typed::DESCS,
    crate::corelib::reflection::fields::typed::DESCS,
    crate::corelib::reflection::methods::typed::DESCS,
    crate::corelib::reflection::properties::typed::DESCS,
    crate::corelib::reflection::attributes::typed::DESCS,
    crate::corelib::reflection::generics::typed::DESCS,
    crate::corelib::reflection::methodof::typed::DESCS,
    crate::corelib::reflection::invoke::typed::DESCS,
    crate::corelib::reflection::accessors::typed::DESCS,
    crate::corelib::reflection::module_load::typed::DESCS,
    crate::corelib::assemblyloadcontext::typed::DESCS,
    crate::corelib::threading::typed::DESCS,
    crate::corelib::io::fs::typed::DESCS,
    crate::corelib::os::process::typed::DESCS,
    crate::corelib::io::typed::DESCS,
    crate::corelib::repl::typed::DESCS,
    crate::corelib::repl::editing::typed::DESCS,
    #[cfg(feature = "native-interop")]
    crate::interop::ext::typed::DESCS,
    crate::corelib::monitor::typed::DESCS,
    #[cfg(feature = "net")]
    crate::corelib::net::tcp::typed::DESCS,
    #[cfg(feature = "net")]
    crate::corelib::net::udp::typed::DESCS,
    #[cfg(not(feature = "net"))]
    crate::corelib::net::unsupported::net::DESCS,
    #[cfg(feature = "tls")]
    crate::corelib::tls::typed::DESCS,
    #[cfg(not(feature = "tls"))]
    crate::corelib::net::unsupported::tls::DESCS,
    crate::corelib::diagnostics::typed::DESCS,
    crate::corelib::array::typed::DESCS,
    #[cfg(test)]
    abi::test_builtins::DESCS,
];

const JOINED: [abi::BuiltinDesc; abi::descs_len(TYPED)] = abi::concat_descs(TYPED);

pub(crate) const BUILTINS: &[abi::BuiltinDesc] = &JOINED;

use crate::vm::runtime::builtins::{BuiltinCatalog, BuiltinInfo};

const fn catalog_entries<const N: usize>(descs: &[abi::BuiltinDesc; N]) -> [BuiltinInfo; N] {
    let mut records = [BuiltinInfo { name: "", frameless: false }; N];
    let mut i = 0;
    while i < N {
        records[i] = BuiltinInfo { name: descs[i].name, frameless: !descs[i].flags.reenters };
        i += 1;
    }
    records
}

const INFO: [BuiltinInfo; JOINED.len()] = catalog_entries(&JOINED);
pub(crate) static CATALOG: BuiltinCatalog = BuiltinCatalog::new(&INFO);
