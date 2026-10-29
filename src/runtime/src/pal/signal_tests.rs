//! Unit tests for `pal::signal` — `sigsafe` write primitives + signal-name
//! table. The actual signal-firing path is covered by the
//! `signal_handler` integration tests.

#![cfg(unix)]

use super::sigsafe;
use super::signal_name;

/// Capture writes by piping into a `pipe(2)` and reading the read end.
/// Lets us assert exact byte sequences without touching real stderr.
fn capture<F: FnOnce(i32)>(f: F) -> Vec<u8> {
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::io::Read;

    let mut fds = [0i32; 2];
    let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "pipe(2) failed");

    f(fds[1]);
    unsafe { libc::close(fds[1]) };

    let mut buf = Vec::new();
    let mut read_end = unsafe { std::fs::File::from(OwnedFd::from_raw_fd(fds[0])) };
    read_end.read_to_end(&mut buf).unwrap();
    buf
}

#[test]
fn write_dec_u32_zero() {
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 0)), b"0");
}

#[test]
fn write_dec_u32_small() {
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 1)),   b"1");
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 9)),   b"9");
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 10)),  b"10");
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 99)),  b"99");
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 100)), b"100");
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, 999)), b"999");
}

#[test]
fn write_dec_u32_max() {
    assert_eq!(capture(|fd| sigsafe::write_dec_u32(fd, u32::MAX)), b"4294967295");
}

#[test]
fn write_hex_u64_zero() {
    assert_eq!(capture(|fd| sigsafe::write_hex_u64(fd, 0)), b"0x0");
}

#[test]
fn write_hex_u64_basic() {
    assert_eq!(capture(|fd| sigsafe::write_hex_u64(fd, 0xff)),       b"0xff");
    assert_eq!(capture(|fd| sigsafe::write_hex_u64(fd, 0xdeadbeef)), b"0xdeadbeef");
    assert_eq!(capture(|fd| sigsafe::write_hex_u64(fd, 0x1000)),     b"0x1000");
}

#[test]
fn write_hex_u64_max() {
    assert_eq!(capture(|fd| sigsafe::write_hex_u64(fd, u64::MAX)), b"0xffffffffffffffff");
}

#[test]
fn write_str_partial_safety() {
    // Splitting input across multiple write_str calls == one big write.
    let chunks: &[&[u8]] = &[b"hello, ", b"signal ", b"world\n"];
    let captured = capture(|fd| {
        for c in chunks {
            sigsafe::write_str(fd, c);
        }
    });
    assert_eq!(captured, b"hello, signal world\n");
}

#[test]
fn signal_name_known() {
    assert_eq!(signal_name(libc::SIGSEGV), b"SIGSEGV");
    assert_eq!(signal_name(libc::SIGABRT), b"SIGABRT");
    assert_eq!(signal_name(libc::SIGFPE),  b"SIGFPE");
    assert_eq!(signal_name(libc::SIGILL),  b"SIGILL");
    assert_eq!(signal_name(libc::SIGBUS),  b"SIGBUS");
}

#[test]
fn signal_name_unknown() {
    assert_eq!(signal_name(libc::SIGUSR1), b"UNKNOWN");
    assert_eq!(signal_name(0),             b"UNKNOWN");
    assert_eq!(signal_name(9999),          b"UNKNOWN");
}
