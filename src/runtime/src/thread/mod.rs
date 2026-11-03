//! Reserved coroutine integration surface.
//!
//! OS threads execute through `corelib::threading`; each owns its VmContext.
//! Contexts of one VM share the core and its registered engine/code lifetime.
//! Coroutine and async execution are not implemented in this module.
