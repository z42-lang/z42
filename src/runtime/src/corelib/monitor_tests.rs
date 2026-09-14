//! `corelib/monitor.rs` 的单测 —— spec「Monitor 原生原语」逐条。
//!
//! 每个会阻塞的用例都带**超时判死锁**：`join` 不能用（它本身会挂住整个测试进程），
//! 所以用共享 flag + 轮询等，超时即断言失败。

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

/// 建一个句柄对象，返回句柄 `Value` 与其中的 `Arc<Monitor>`。
fn new_monitor(c: &VmContext) -> (Value, Arc<Monitor>) {
    let h = builtin_monitor_new(c, &[]).unwrap();
    let m = monitor_arg(&[h.clone()], "test").unwrap();
    (h, m)
}

fn wait_until(deadline: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    cond()
}

#[test]
fn new_returns_monitor_handle() {
    let c = ctx();
    let h = builtin_monitor_new(&c, &[]).unwrap();
    match &h {
        Value::Object(rc) => assert!(matches!(rc.borrow().native(), NativeData::Monitor(_))),
        other => panic!("expected an object handle, got {other:?}"),
    }
}

#[test]
fn non_handle_argument_errors() {
    let c = ctx();
    let err = builtin_monitor_enter(&c, &[Value::I64(3)]).unwrap_err();
    assert!(err.to_string().contains("expected a monitor handle"));
    let err = builtin_monitor_exit(&c, &[]).unwrap_err();
    assert!(err.to_string().contains("missing monitor handle"));
}

#[test]
fn enter_then_exit_round_trip() {
    let c = ctx();
    let (h, _m) = new_monitor(&c);
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    builtin_monitor_exit(&c, &[h.clone()]).unwrap();
    // 释放后可以再次进入（不是一次性的）。
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    builtin_monitor_exit(&c, &[h]).unwrap();
}

#[test]
fn reentrant_enter_errors_instead_of_deadlocking() {
    let c = ctx();
    let (h, m) = new_monitor(&c);
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    let err = builtin_monitor_enter(&c, &[h.clone()]).unwrap_err();
    assert!(err.to_string().contains("not reentrant"), "got: {err}");
    // 报错不改变持有状态：仍由本线程持有，exit 成功。
    assert!(!m.try_enter());
    builtin_monitor_exit(&c, &[h]).unwrap();
}

#[test]
fn exit_by_non_owner_errors() {
    let c = ctx();
    let (h, m) = new_monitor(&c);
    // 从未持有。
    let err = builtin_monitor_exit(&c, &[h.clone()]).unwrap_err();
    assert!(err.to_string().contains("does not hold the monitor"), "got: {err}");
    // 由别的线程持有。
    let m2 = Arc::clone(&m);
    let holder = std::thread::spawn(move || m2.try_enter());
    assert!(holder.join().unwrap());
    let err = builtin_monitor_exit(&c, &[h]).unwrap_err();
    assert!(err.to_string().contains("does not hold the monitor"), "got: {err}");
}

#[test]
fn wait_by_non_owner_errors() {
    let c = ctx();
    let (h, _m) = new_monitor(&c);
    let err = builtin_monitor_wait(&c, &[h]).unwrap_err();
    assert!(err.to_string().contains("does not hold the monitor"), "got: {err}");
}

#[test]
fn try_enter_fails_while_held_by_another_thread() {
    let c = ctx();
    let (_h, m) = new_monitor(&c);
    let m2 = Arc::clone(&m);
    let held = Arc::new(AtomicU64::new(0));
    let held2 = Arc::clone(&held);
    let worker = std::thread::spawn(move || {
        assert!(m2.try_enter());
        held2.store(1, Ordering::SeqCst);
        while held2.load(Ordering::SeqCst) == 1 {
            std::thread::sleep(Duration::from_millis(1));
        }
        m2.exit().unwrap();
    });
    assert!(wait_until(Duration::from_secs(5), || held.load(Ordering::SeqCst) == 1));
    assert!(!m.try_enter(), "try_enter must not succeed while another thread holds it");
    held.store(2, Ordering::SeqCst);
    worker.join().unwrap();
    assert!(m.try_enter(), "released monitor must be acquirable");
}

#[test]
fn try_enter_fails_while_held_by_this_thread() {
    let c = ctx();
    let (_h, m) = new_monitor(&c);
    assert!(m.try_enter());
    assert!(!m.try_enter(), "try_enter is not reentrant either");
}

#[test]
fn contended_enter_serialises_two_threads() {
    let c = ctx();
    let (_h, m) = new_monitor(&c);
    let core = Arc::clone(&c.core);
    let counter = Arc::new(AtomicU64::new(0));
    let mut workers = Vec::new();
    for _ in 0..4 {
        let m2 = Arc::clone(&m);
        let core2 = Arc::clone(&core);
        let counter2 = Arc::clone(&counter);
        workers.push(std::thread::spawn(move || {
            let w = VmContext::new_with_core(core2);
            for _ in 0..200 {
                m2.enter(&w).unwrap();
                // 非原子的读-改-写：没有互斥就会丢更新。
                let v = counter2.load(Ordering::Relaxed);
                counter2.store(v + 1, Ordering::Relaxed);
                m2.exit().unwrap();
            }
        }));
    }
    for w in workers {
        w.join().unwrap();
    }
    assert_eq!(counter.load(Ordering::SeqCst), 800);
}

#[test]
fn wait_releases_and_reacquires() {
    let c = ctx();
    let (h, m) = new_monitor(&c);
    let core = Arc::clone(&c.core);
    // 阶段：0 初始 → 1 worker 已持有并在 wait → 2 主线程已 enter 过（证明 wait 放开了）
    //      → 3 worker 从 wait 返回
    let stage = Arc::new(AtomicU64::new(0));
    let m2 = Arc::clone(&m);
    let stage2 = Arc::clone(&stage);
    let worker = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core);
        m2.enter(&w).unwrap();
        stage2.store(1, Ordering::SeqCst);
        while stage2.load(Ordering::SeqCst) < 2 {
            m2.wait(&w).unwrap();
        }
        // 从 wait 返回时必须重新持有 —— 此刻别的线程不可能进得来。
        assert!(!m2.try_enter());
        stage2.store(3, Ordering::SeqCst);
        m2.exit().unwrap();
    });
    assert!(wait_until(Duration::from_secs(5), || stage.load(Ordering::SeqCst) >= 1));
    // worker 在 wait 里 ⇒ 主线程能进（这就是「wait 释放了持有」）。
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    stage.store(2, Ordering::SeqCst);
    builtin_monitor_exit(&c, &[h]).unwrap();     // exit 唤醒 wait 者
    assert!(wait_until(Duration::from_secs(5), || stage.load(Ordering::SeqCst) == 3),
            "worker did not return from wait (stage={})", stage.load(Ordering::SeqCst));
    worker.join().unwrap();
}

#[test]
fn two_waiters_do_not_spin_against_each_other() {
    // 回归：若「等进入」与「等信号」共用一个条件变量，两个 wait 者在各自释放持有时会互相唤醒 →
    // 醒来条件不满足再 wait → 再唤醒对方……无限空转。判据：统计 wait 返回的次数。两个 worker
    // 都进入 wait 之后，**没有任何 exit** 的安静期里它们不该被唤醒；空转实现会是成千上万次。
    let c = ctx();
    let (h, m) = new_monitor(&c);
    let core = Arc::clone(&c.core);
    let released = Arc::new(AtomicU64::new(0));
    let wakeups = Arc::new(AtomicU64::new(0));
    let woke = Arc::new(AtomicU64::new(0));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let m2 = Arc::clone(&m);
        let core2 = Arc::clone(&core);
        let released2 = Arc::clone(&released);
        let wakeups2 = Arc::clone(&wakeups);
        let woke2 = Arc::clone(&woke);
        workers.push(std::thread::spawn(move || {
            let w = VmContext::new_with_core(core2);
            m2.enter(&w).unwrap();
            while released2.load(Ordering::SeqCst) == 0 {
                m2.wait(&w).unwrap();
                wakeups2.fetch_add(1, Ordering::SeqCst);
            }
            woke2.fetch_add(1, Ordering::SeqCst);
            m2.exit().unwrap();
        }));
    }
    // 两个都在 wait 里 ⇔ parked_count == 2。
    assert!(wait_until(Duration::from_secs(5), || core.parked_count.load(Ordering::Acquire) == 2));
    let before = wakeups.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(100));
    let during_quiet = wakeups.load(Ordering::SeqCst) - before;
    assert!(during_quiet == 0, "waiters woke {during_quiet} times with no exit — they are spinning");
    released.store(1, Ordering::SeqCst);
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    builtin_monitor_exit(&c, &[h]).unwrap();     // 广播：两个 wait 者都该出来
    assert!(wait_until(Duration::from_secs(5), || woke.load(Ordering::SeqCst) == 2),
            "waiters stuck: woke={}", woke.load(Ordering::SeqCst));
    for w in workers {
        w.join().unwrap();
    }
}

#[test]
fn blocked_enter_is_parked_for_gc() {
    // spec：阻塞在 enter 上的线程必须处于 park 状态（计入 `parked_count`），GC 才不必等它到达
    // 字节码 safepoint。判据直接看 park 计数：阻塞期间 == 1，拿到锁后回到 0。
    let c = ctx();
    let (_h, m) = new_monitor(&c);
    let core = Arc::clone(&c.core);
    assert!(m.try_enter());                      // 主线程持有，让 worker 必须阻塞
    let m2 = Arc::clone(&m);
    let core2 = Arc::clone(&core);
    let acquired = Arc::new(AtomicU64::new(0));
    let acquired2 = Arc::clone(&acquired);
    let worker = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core2);
        m2.enter(&w).unwrap();
        acquired2.store(1, Ordering::SeqCst);
        m2.exit().unwrap();
    });
    assert!(wait_until(Duration::from_secs(5), || core.parked_count.load(Ordering::Acquire) == 1),
            "a thread blocked in Monitor::enter must be parked");
    assert_eq!(acquired.load(Ordering::SeqCst), 0);
    m.exit().unwrap();
    assert!(wait_until(Duration::from_secs(5), || acquired.load(Ordering::SeqCst) == 1));
    worker.join().unwrap();
    assert_eq!(core.parked_count.load(Ordering::Acquire), 0, "park must be released");
}

#[test]
fn uncontended_enter_does_not_park() {
    // 快路径不 park（park 的进出要拿全局 phase 锁并 notify_all）。
    let c = ctx();
    let (h, _m) = new_monitor(&c);
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    assert_eq!(c.core.parked_count.load(Ordering::Acquire), 0);
    builtin_monitor_exit(&c, &[h]).unwrap();
}

#[test]
fn waiting_thread_is_parked_for_gc() {
    let c = ctx();
    let (h, m) = new_monitor(&c);
    let core = Arc::clone(&c.core);
    let go = Arc::new(AtomicU64::new(0));
    let m2 = Arc::clone(&m);
    let core2 = Arc::clone(&core);
    let go2 = Arc::clone(&go);
    let worker = std::thread::spawn(move || {
        let w = VmContext::new_with_core(core2);
        m2.enter(&w).unwrap();
        while go2.load(Ordering::SeqCst) == 0 {
            m2.wait(&w).unwrap();
        }
        m2.exit().unwrap();
    });
    assert!(wait_until(Duration::from_secs(5), || core.parked_count.load(Ordering::Acquire) == 1),
            "a thread blocked in Monitor::wait must be parked");
    go.store(1, Ordering::SeqCst);
    builtin_monitor_enter(&c, &[h.clone()]).unwrap();
    builtin_monitor_exit(&c, &[h]).unwrap();
    worker.join().unwrap();
    assert_eq!(core.parked_count.load(Ordering::Acquire), 0, "park must be released");
}
