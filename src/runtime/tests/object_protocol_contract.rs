//! Compile the real domain contract with no execution/context modules available.
mod object {
    #[derive(Debug, PartialEq)]
    pub struct Value(pub usize);
}
#[path = "../src/object/protocol.rs"]
mod protocol;
use object::Value;
use protocol::ObjectProtocol;

// Neither type implements Clone or Copy; the protocol must not acquire these bounds.
struct Context(Vec<u8>);
struct DomainError;
fn get_type(context: &Context, value: &Value) -> Result<Value, DomainError> {
    Ok(Value(context.0.len() + value.0))
}
fn text(value: &Value) -> Result<String, DomainError> { Ok(value.0.to_string()) }
fn named(context: &Context, name: &str) -> Value { Value(context.0.len() + name.len()) }
fn constructed(_: &Context, _: &str, args: &[String]) -> Value { Value(args.len()) }
fn capabilities() -> ObjectProtocol<Context, DomainError> {
    ObjectProtocol { get_type, default_to_string: text, make_type_from_name: named, make_constructed_type: constructed }
}

#[test]
fn non_copy_context_and_error_do_not_prevent_copying_the_contract() {
    fn requires_copy<T: Copy>(_: T) {}
    let protocol = capabilities();
    requires_copy(protocol);
    let copied = protocol;
    let context = Context(vec![1, 2]);
    assert!(matches!((protocol.get_type)(&context, &Value(3)), Ok(Value(5))));
    assert!(matches!((copied.default_to_string)(&Value(7)), Ok(text) if text == "7"));
    assert_eq!((copied.make_type_from_name)(&context, "Type"), Value(6));
    assert_eq!((copied.make_constructed_type)(&context, "Box", &["Int64".into()]), Value(1));
}

#[test]
fn contract_stores_only_four_function_pointers() {
    assert_eq!(std::mem::size_of::<ObjectProtocol<Context, DomainError>>(), 4 * std::mem::size_of::<fn()>());
    assert_eq!(std::mem::align_of::<ObjectProtocol<Context, DomainError>>(), std::mem::align_of::<fn()>());
    assert!(!std::mem::needs_drop::<ObjectProtocol<Context, DomainError>>());
}
