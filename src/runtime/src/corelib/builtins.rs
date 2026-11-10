//! Core-library descriptors. Provider assembly belongs to the host.
use crate::exec::call::builtin_abi as abi;

const TYPED: &[&[abi::BuiltinDesc]] = &[
    crate::corelib::core::convert::typed::DESCS,
    crate::corelib::core::math::typed::DESCS,
    crate::corelib::core::object::typed::DESCS,
    crate::corelib::os::platform::typed::DESCS,
    crate::corelib::os::system::typed::DESCS,
    crate::corelib::core::config::typed::DESCS,
    crate::corelib::core::appprops::typed::DESCS,
    crate::corelib::core::symavail::typed::DESCS,
    crate::corelib::core::bench::typed::DESCS,
    crate::corelib::core::crypto::typed::DESCS,
    crate::corelib::core::gc::typed::DESCS,
    crate::corelib::core::char::typed::DESCS,
    crate::corelib::core::string::typed::DESCS,
    crate::corelib::core::reflection::enums::typed::DESCS,
    crate::corelib::core::reflection::type_query::typed::DESCS,
    crate::corelib::core::reflection::fields::typed::DESCS,
    crate::corelib::core::reflection::methods::typed::DESCS,
    crate::corelib::core::reflection::properties::typed::DESCS,
    crate::corelib::core::reflection::attributes::typed::DESCS,
    crate::corelib::core::reflection::generics::typed::DESCS,
    crate::corelib::core::reflection::methodof::typed::DESCS,
    crate::corelib::core::reflection::invoke::typed::DESCS,
    crate::corelib::core::reflection::accessors::typed::DESCS,
    crate::corelib::core::reflection::module_load::typed::DESCS,
    crate::corelib::core::reflection::static_value_builtins::DESCS,
    crate::corelib::core::assemblyloadcontext::typed::DESCS,
    crate::corelib::core::threading::typed::DESCS,
    crate::corelib::io::fs::typed::DESCS,
    crate::corelib::os::process::typed::DESCS,
    crate::corelib::io::typed::DESCS,
    crate::corelib::core::monitor::typed::DESCS,
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
    crate::corelib::core::diagnostics::typed::DESCS,
    crate::corelib::core::array::typed::DESCS,
];

const JOINED: [abi::BuiltinDesc; abi::descs_len(TYPED)] = abi::concat_descs(TYPED);

pub(crate) const DESCS: &[abi::BuiltinDesc] = &JOINED;
