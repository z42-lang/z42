//! `Std.Net` builtins — TCP 连接 / 监听 / 收发 / 超时 / 句柄释放（非 wasm32）。refactor-split-network（2026-09-03）：自 `network.rs`
//! 内联 `mod imp` 逐行搬出；`KIND_*` / 句柄槽等共享定义仍在 hub（`use super::*`）。

#![allow(unused_imports)]
use super::*;
use std::net::{TcpListener, TcpStream, SocketAddr, ToSocketAddrs};
use std::io::{Read, Write};

pub fn builtin_net_tcp_connect(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_connect";
    let host = arg_str(args, 0, NAME)?.to_string();
    let port = require_port(args, 1, NAME)?;

    let addr = format!("{}:{}", host, port);
    match TcpStream::connect(&addr) {
        Ok(stream) => {
            let slot_id = ctx.alloc_tcp_socket_slot(stream);
            Ok(ok_value(ctx, slot_id as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("connect to {}: {}", addr, e))),
    }
}

pub fn builtin_net_tcp_listen(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_listen";
    let host = arg_str(args, 0, NAME)?.to_string();
    let port = require_port(args, 1, NAME)?;

    let bind_target = format!("{}:{}", host, port);
    let bind_result = bind_target.to_socket_addrs()
        .and_then(|mut iter| iter.next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "no addresses")))
        .and_then(|addr: SocketAddr| TcpListener::bind(addr));

    match bind_result {
        Ok(listener) => {
            let actual_port = listener.local_addr()
                .map(|a| a.port())
                .unwrap_or(port);
            let slot_id = ctx.alloc_tcp_listener_slot(listener);
            Ok(ok_two(ctx, slot_id as i64, actual_port as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("bind {}: {}", bind_target, e))),
    }
}

pub fn builtin_net_tcp_accept(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_accept";
    let slot_id = require_slot_id(args, 0, NAME)?;

    // Take the listener out so `.accept()` can block without holding
    // the global listener table lock.
    let listener = {
        let mut map = ctx.core.tcp_listeners.lock();
        map.remove(&slot_id)
    };
    let Some(listener) = listener else {
        return Ok(handle_invalid(ctx));
    };

    let accept_result = listener.accept();
    // Put listener back so subsequent Accept calls work.
    ctx.core.tcp_listeners.lock().insert(slot_id, listener);

    match accept_result {
        Ok((stream, _peer)) => {
            let sock_slot = ctx.alloc_tcp_socket_slot(stream);
            Ok(ok_value(ctx, sock_slot as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("accept: {}", e))),
    }
}

pub fn builtin_net_tcp_socket_read(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_read";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let buf_arr = match args.get(1) {
        Some(Value::Array(rc)) => rc.clone(),
        other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
    };
    let offset = arg_i64(args, 2, NAME)? as usize;
    let count  = arg_i64(args, 3, NAME)? as usize;

    let buf_len = buf_arr.borrow().len();
    if offset + count > buf_len {
        bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
    }
    if count == 0 { return Ok(ok_value(ctx, 0)); }

    let stream = {
        let mut map = ctx.core.tcp_sockets.lock();
        map.remove(&slot_id)
    };
    let Some(mut stream) = stream else {
        return Ok(handle_invalid(ctx));
    };

    let mut tmp = vec![0u8; count];
    let read_result = stream.read(&mut tmp);

    ctx.core.tcp_sockets.lock().insert(slot_id, stream);

    match read_result {
        Ok(n) => {
            let mut borrowed = buf_arr.borrow_mut();
            for i in 0..n {
                borrowed.set_boxed(offset + i, Value::I64(tmp[i] as i64));
            }
            Ok(ok_value(ctx, n as i64))
        }
        Err(e) => Ok(socket_err(ctx, format!("read: {}", e))),
    }
}

pub fn builtin_net_tcp_socket_write(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_write";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let buf_arr = match args.get(1) {
        Some(Value::Array(rc)) => rc.clone(),
        other => bail!("{}: arg 1 expected byte array, got {:?}", NAME, other),
    };
    let offset = arg_i64(args, 2, NAME)? as usize;
    let count  = arg_i64(args, 3, NAME)? as usize;

    let buf_len = buf_arr.borrow().len();
    if offset + count > buf_len {
        bail!("{}: offset {} + count {} exceeds buf length {}", NAME, offset, count, buf_len);
    }
    if count == 0 { return Ok(ok_value(ctx, 0)); }

    let mut tmp = vec![0u8; count];
    {
        let borrowed = buf_arr.borrow();
        // packed-primitive-arrays Step 3: packed `Bytes` → slice-copy the send
        // window in one memcpy, no per-byte unbox.
        if let Some(b) = borrowed.as_bytes() {
            tmp.copy_from_slice(&b[offset..offset + count]);
        } else {
            for i in 0..count {
                match borrowed.get_boxed(offset + i) {
                    Value::I64(v) => tmp[i] = (v & 0xFF) as u8,
                    other => bail!("{}: byte[] elem at {} expected I64, got {:?}", NAME, offset + i, other),
                }
            }
        }
    }

    let stream = {
        let mut map = ctx.core.tcp_sockets.lock();
        map.remove(&slot_id)
    };
    let Some(mut stream) = stream else {
        return Ok(handle_invalid(ctx));
    };

    let write_result = stream.write_all(&tmp).map(|_| count);

    ctx.core.tcp_sockets.lock().insert(slot_id, stream);

    match write_result {
        Ok(n) => Ok(ok_value(ctx, n as i64)),
        Err(e) => Ok(socket_err(ctx, format!("write: {}", e))),
    }
}

pub fn builtin_net_tcp_socket_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_drop";
    let slot_id = require_slot_id(args, 0, NAME)?;
    ctx.core.tcp_sockets.lock().remove(&slot_id);
    Ok(Value::Null)
}

// add-httpclient-timeout (2026-05-27): apply read / write deadlines so
// a misbehaving peer can't hang the script. `millis <= 0` clears the
// timeout (blocking I/O). On error returns socket_err; on missing slot
// returns handle_invalid (caller treats as already-disposed).

fn apply_timeout(
    ctx: &VmContext,
    slot_id: u64,
    millis: i64,
    which: &'static str,
) -> Result<Value> {
    let dur = if millis > 0 {
        Some(std::time::Duration::from_millis(millis as u64))
    } else {
        None
    };
    let stream = {
        let map = ctx.core.tcp_sockets.lock();
        match map.get(&slot_id) {
            Some(s) => s.try_clone(),
            None => return Ok(handle_invalid(ctx)),
        }
    };
    let stream = match stream {
        Ok(s) => s,
        Err(e) => return Ok(socket_err(ctx, format!("{}: try_clone: {}", which, e))),
    };
    let result = if which == "set_read_timeout" {
        stream.set_read_timeout(dur)
    } else {
        stream.set_write_timeout(dur)
    };
    match result {
        Ok(()) => Ok(ok_value(ctx, 0)),
        Err(e) => Ok(socket_err(ctx, format!("{}: {}", which, e))),
    }
}

pub fn builtin_net_tcp_socket_set_read_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_set_read_timeout";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let millis = arg_i64(args, 1, NAME)?;
    apply_timeout(ctx, slot_id, millis, "set_read_timeout")
}

pub fn builtin_net_tcp_socket_set_write_timeout(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_socket_set_write_timeout";
    let slot_id = require_slot_id(args, 0, NAME)?;
    let millis = arg_i64(args, 1, NAME)?;
    apply_timeout(ctx, slot_id, millis, "set_write_timeout")
}

pub fn builtin_net_tcp_listener_drop(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__net_tcp_listener_drop";
    let slot_id = require_slot_id(args, 0, NAME)?;
    ctx.core.tcp_listeners.lock().remove(&slot_id);
    Ok(Value::Null)
}
