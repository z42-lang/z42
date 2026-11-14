//! Generic native module dispatch. Provider implementation and state stay in its crate.
use super::{
    module::{self, ModuleInstance, NativeModule},
    native_value::{self, Arena, Owned},
};
use crate::exec::call::builtin_abi::{BuiltinError, CallResult};
use crate::{context::VmContext, object::Value};
use std::{ffi::c_void, sync::Arc};
use z42_abi::module as abi;

struct Frame<'a> {
    ctx: &'a VmContext,
    arguments: Vec<Owned>,
    arena: Arena,
    result: Option<Owned>,
    error: Option<BuiltinError>,
    blocking: bool,
    reenters: bool,
}

fn instance(ctx: &VmContext, module: Arc<NativeModule>) -> anyhow::Result<Arc<ModuleInstance>> {
    let state = super::state::state(ctx)?;
    if let Some(instance) = state.instances.read().get(module.name).cloned() {
        return Ok(instance);
    }
    let name = module.name;
    // Provider constructors run outside registry locks. A race loser drops its own state.
    let created = Arc::new(ModuleInstance::create(module)?);
    let instance = {
        let mut instances = state.instances.write();
        instances
            .entry(name)
            .or_insert_with(|| created.clone())
            .clone()
    };
    // A race loser's native destructor runs only after releasing the registry lock.
    Ok(instance)
}

pub(crate) fn invoke_export(
    ctx: &VmContext,
    provider: Option<&str>,
    name: &str,
    arguments: &[Value],
) -> Option<CallResult> {
    let state = match super::state::state(ctx) {
        Ok(state) => state,
        Err(error) => return Some(Err(Box::new(BuiltinError::Internal(error.to_string())))),
    };
    let registry = state.providers;
    if let Some(provider) = provider {
        if let Some(id) = registry.resolve(provider, name) {
            return Some((registry.functions()[id.0 as usize].call_slice)(ctx, arguments));
        }
    }
    let module = match provider {
        Some(provider) => state
            .external_modules
            .read()
            .get(provider)
            .cloned()
            .or_else(|| registry.module(provider)),
        None => {
            if registry
                .functions()
                .iter()
                .any(|function| function.name == name)
            {
                return None;
            }
            let external = state.external_modules.read();
            let mut modules: Vec<_> = external
                .values()
                .filter(|module| module.function(name).is_some())
                .cloned()
                .collect();
            modules.extend(
                registry
                    .modules()
                    .filter(|module| {
                        !external.contains_key(module.name) && module.function(name).is_some()
                    })
                    .cloned(),
            );
            if modules.len() != 1 {
                return None;
            }
            modules.pop()
        }
    }?;
    let function = module.function(name)?;
    let name = match unsafe { module::text(function.name) } {
        Ok(name) => name,
        Err(error) => return Some(Err(Box::new(BuiltinError::Internal(error.to_string())))),
    };
    Some((|| -> CallResult {
        if arguments.len() != function.parameters.len {
            return Err(Box::new(BuiltinError::arity(
                name,
                function.parameters.len,
                arguments.len(),
            )));
        }
        let parameters = unsafe { module::span(function.parameters.data, function.parameters.len) }
            .map_err(|error| Box::new(BuiltinError::Internal(error.to_string())))?;
        let arguments: Result<Vec<_>, _> = arguments
            .iter()
            .zip(parameters)
            .enumerate()
            .map(|(index, (value, &kind))| native_value::from_vm(value, kind, name, index))
            .collect();
        let arguments = arguments.map_err(Box::new)?;
        let module = module::materialize(ctx, module).map_err(|error| {
            Box::new(BuiltinError::NotSupported(format!(
                "native module `{name}` unavailable: {error:#}"
            )))
        })?;
        let function = module.function(name).ok_or_else(|| {
            Box::new(BuiltinError::Internal(
                "native implementation lost a validated export".into(),
            ))
        })?;
        let instance = if module.has_state() {
            Some(
                instance(ctx, module)
                    .map_err(|error| Box::new(BuiltinError::Internal(error.to_string())))?,
            )
        } else {
            None
        };
        let mut frame = Frame {
            ctx,
            arguments,
            arena: Arena::default(),
            result: None,
            error: None,
            blocking: function.flags & abi::BLOCKING != 0,
            reenters: function.flags & abi::REENTERS != 0,
        };
        let console = crate::pal::console::capabilities();
        let host = abi::HostApi {
            capabilities: HOST.capabilities
                | if console.input {
                    abi::CAP_CONSOLE_INPUT
                } else {
                    0
                }
                | if console.terminal {
                    abi::CAP_TERMINAL
                } else {
                    0
                },
            ..HOST
        };
        let mut call = abi::Call {
            host: &host,
            context: (&mut frame as *mut Frame<'_>).cast(),
            state: instance
                .as_ref()
                .map_or(std::ptr::null_mut(), |instance| instance.state),
        };
        let park = frame
            .blocking
            .then(|| crate::threads::safepoint::NativeParkGuard::enter(ctx));
        let status = match function.invoke {
            Some(invoke) => unsafe { invoke(&mut call) },
            None => abi::ERROR,
        };
        drop(park);
        if status == abi::THROWN {
            return Err(Box::new(match ctx.take_pending_thrown() {
                Some(value) => BuiltinError::Thrown(value),
                None => BuiltinError::Internal(
                    "native module returned THROWN without a script exception".into(),
                ),
            }));
        }
        if status != abi::OK {
            let _ = ctx.take_pending_thrown();
            return Err(Box::new(frame.error.unwrap_or_else(|| {
                BuiltinError::Internal("native module failed without an error".into())
            })));
        }
        // Editing callbacks may deliberately swallow a script exception.
        let _ = ctx.take_pending_thrown();
        let result = frame.result.ok_or_else(|| {
            Box::new(BuiltinError::Internal(
                "native module did not return a value or void".into(),
            ))
        })?;
        let kind = match &result {
            Owned::Void => abi::VOID,
            Owned::Null => abi::NULL,
            Owned::I64(_) => abi::I64,
            Owned::F64(_) => abi::F64,
            Owned::Bool(_) => abi::BOOL,
            Owned::Str(_) => abi::STR,
            Owned::Bytes(_) => abi::BYTES,
            Owned::Strings(_) => abi::STR_ARRAY,
        };
        let compatible = kind == function.result
            || function.result == abi::OPT_STR && matches!(kind, abi::NULL | abi::STR)
            || kind == abi::NULL && function.result == abi::STR_ARRAY;
        if !compatible {
            return Err(Box::new(BuiltinError::Internal(format!(
                "native result kind {kind} does not match declared kind {}",
                function.result
            ))));
        }
        native_value::to_vm(ctx, result)
    })())
}

unsafe fn frame<'a>(call: *mut abi::Call) -> Option<&'a mut Frame<'a>> {
    let call = unsafe { call.as_ref()? };
    unsafe { call.context.cast::<Frame<'a>>().as_mut() }
}

unsafe extern "C" fn argument(call: *mut abi::Call, index: usize, output: *mut abi::Value) -> u32 {
    let (Some(frame), Some(output)) = (unsafe { frame(call) }, unsafe { output.as_mut() }) else {
        return abi::ERROR;
    };
    match frame.arguments.get(index) {
        Some(value) => {
            *output = frame.arena.view(value);
            abi::OK
        }
        None => {
            frame.error = Some(BuiltinError::ArgumentOutOfRange(
                "native argument index is out of range".into(),
            ));
            abi::ERROR
        }
    }
}
unsafe extern "C" fn return_value(call: *mut abi::Call, value: *const abi::Value) -> u32 {
    let (Some(frame), Some(value)) = (unsafe { frame(call) }, unsafe { value.as_ref() }) else {
        return abi::ERROR;
    };
    match unsafe { z42_rs::module::decode(value) } {
        Ok(value) => {
            frame.result = Some(value);
            frame.error = None;
            abi::OK
        }
        Err(error) => {
            frame.error = Some(native_value::error(error));
            abi::ERROR
        }
    }
}
unsafe extern "C" fn raise(call: *mut abi::Call, class: abi::Text, message: abi::Text) -> u32 {
    let Some(frame) = (unsafe { frame(call) }) else {
        return abi::ERROR;
    };
    let error = match (unsafe { module::text(class) }, unsafe {
        module::text(message)
    }) {
        (Ok(class), Ok(message)) => BuiltinError::ExceptionOwned {
            class: class.to_owned(),
            message: message.to_owned(),
        },
        _ => BuiltinError::Internal("invalid native error text".into()),
    };
    if frame.error.is_none() {
        frame.error = Some(error);
    }
    abi::ERROR
}

struct Roots<'a> {
    ctx: &'a VmContext,
    handles: Vec<crate::gc::RootHandle>,
}
impl Drop for Roots<'_> {
    fn drop(&mut self) {
        for handle in &self.handles {
            self.ctx.heap().unpin_root(*handle);
        }
    }
}

unsafe extern "C" fn invoke_script(
    call: *mut abi::Call,
    name: abi::Text,
    arguments: *const abi::Value,
    count: usize,
    output: *mut abi::Value,
) -> u32 {
    let (Some(frame), Some(output)) = (unsafe { frame(call) }, unsafe { output.as_mut() }) else {
        return abi::ERROR;
    };
    if !frame.reenters {
        frame.error = Some(BuiltinError::Internal(
            "native callback requires the reenters flag".into(),
        ));
        return abi::ERROR;
    }
    let resume = frame
        .blocking
        .then(|| crate::threads::safepoint::NativeUnparkGuard::exit(frame.ctx));
    let result =
        (|| -> Result<abi::Value, BuiltinError> {
            let name = unsafe { module::text(name) }
                .map_err(|error| BuiltinError::Argument(error.to_string()))?;
            let raw = unsafe { module::span(arguments, count) }
                .map_err(|error| BuiltinError::Argument(error.to_string()))?;
            let mut roots = Roots {
                ctx: frame.ctx,
                handles: Vec::new(),
            };
            let mut values = Vec::new();
            for raw in raw {
                let owned = unsafe { z42_rs::module::decode(raw) }.map_err(native_value::error)?;
                let value = native_value::to_vm(frame.ctx, owned)
                    .map_err(|error| *error)?
                    .ok_or_else(|| BuiltinError::Argument("void callback argument".into()))?;
                roots.handles.push(frame.ctx.heap().pin_root(value));
                values.push(value);
            }
            let module = frame.ctx.core().runtime().module.as_ref().ok_or_else(|| {
                BuiltinError::InvalidOperation("callback VM has no module".into())
            })?;
            let loaded;
            let function = match module.func_index.get(name) {
                Some(&index) => &module.functions[index],
                None => {
                    loaded = frame.ctx.try_lookup_function(name).ok_or_else(|| {
                        BuiltinError::Exception {
                            class: "Std.MissingSymbolException",
                            message: format!("native callback `{name}` not found"),
                        }
                    })?;
                    loaded.as_ref()
                }
            };
            if function.param_count != values.len() {
                return Err(BuiltinError::arity(
                    "native callback",
                    function.param_count,
                    values.len(),
                ));
            }
            match crate::exec::engine::invoke(frame.ctx, module, function, &values)
                .map_err(BuiltinError::Legacy)?
            {
                crate::exec::call::outcome::ExecOutcome::Returned(value) => {
                    let owned = match value {
                        Some(value) => native_value::from_callback(&value)?,
                        None => Owned::Void,
                    };
                    Ok(frame.arena.insert(owned))
                }
                crate::exec::call::outcome::ExecOutcome::Thrown(value) => {
                    frame.ctx.set_pending_thrown(value);
                    Err(BuiltinError::Thrown(value))
                }
            }
        })();
    drop(resume);
    match result {
        Ok(value) => {
            *output = value;
            abi::OK
        }
        Err(BuiltinError::Thrown(_)) => abi::THROWN,
        Err(error) => {
            frame.error = Some(error);
            abi::ERROR
        }
    }
}

unsafe extern "C" fn resolve_symbol(
    call: *mut abi::Call,
    library: abi::Text,
    symbol: abi::Text,
    output: *mut *mut c_void,
) -> u32 {
    let (Some(frame), Some(output)) = (unsafe { frame(call) }, unsafe { output.as_mut() }) else {
        return abi::ERROR;
    };
    let result = (|| -> anyhow::Result<*mut c_void> {
        let library = unsafe { module::text(library)? };
        let symbol = unsafe { module::text(symbol)? };
        crate::loader::native::resolve_symbol(frame.ctx, library, symbol)
    })();
    match result {
        Ok(symbol) => {
            *output = symbol;
            abi::OK
        }
        Err(error) => {
            frame.error = Some(BuiltinError::NotSupported(format!("{error:#}")));
            abi::ERROR
        }
    }
}

static HOST: abi::HostApi = abi::HostApi {
    version: abi::MODULE_ABI_VERSION,
    size: std::mem::size_of::<abi::HostApi>() as u32,
    capabilities: (if crate::pal::dynlib::is_supported() {
        abi::CAP_NATIVE_LOADING
    } else {
        0
    }) | (if crate::pal::fs::HAS_FS {
        abi::CAP_FILESYSTEM
    } else {
        0
    }),
    argument,
    return_value,
    raise,
    invoke_script,
    resolve_symbol,
};

#[cfg(test)]
#[path = "module_tests.rs"]
mod tests;
