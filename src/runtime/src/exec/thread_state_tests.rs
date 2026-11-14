use crate::{exec::{engine::EngineRegistry, ThreadExecState}, metadata::statics::CctorRegistry, object::Value};
use std::sync::Arc;

#[test]
fn exception_handoff_needs_only_thread_execution_state() {
    let cctors = Arc::new(CctorRegistry::default());
    let engines = EngineRegistry::new(None);
    let first = ThreadExecState::new(&cctors, &engines);
    let second = ThreadExecState::new(&cctors, &engines);

    first.set_exception(Value::I64(11));
    first.set_pending_thrown(Value::I64(22));
    assert_eq!(first.peek_exception(), Some(Value::I64(11)));
    assert_eq!(first.peek_exception(), Some(Value::I64(11)));
    assert_eq!(second.take_exception(), None);
    assert_eq!(second.take_pending_thrown(), None);
    assert_eq!(first.take_pending_thrown(), Some(Value::I64(22)));
    assert_eq!(first.take_pending_thrown(), None);
    assert_eq!(first.take_exception(), Some(Value::I64(11)));
    assert_eq!(first.peek_exception(), None);
    assert_eq!(first.take_exception(), None);
}
