use super::*;

#[test]
fn test_empty_stack() {
    let stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    assert_eq!(stack.current_fqn(), "");
    assert!(stack.is_empty());
    assert_eq!(stack.depth(), 0);
}

#[test]
fn test_push_single_scope() {
    let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    stack.push("my_crate");
    assert_eq!(stack.current_fqn(), "my_crate");
    assert_eq!(stack.depth(), 1);
}

#[test]
fn test_push_multiple_scopes() {
    let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    stack.push("my_crate");
    stack.push("my_module");
    stack.push("MyStruct");
    assert_eq!(stack.current_fqn(), "my_crate::my_module::MyStruct");
    assert_eq!(stack.depth(), 3);
}

#[test]
fn test_pop_scope() {
    let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    stack.push("my_crate");
    stack.push("my_module");
    assert_eq!(stack.pop(), Some("my_module".to_string()));
    assert_eq!(stack.current_fqn(), "my_crate");
    assert_eq!(stack.pop(), Some("my_crate".to_string()));
    assert!(stack.is_empty());
}

#[test]
fn test_fqn_for_symbol() {
    let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    assert_eq!(stack.fqn_for_symbol("top_level_fn"), "top_level_fn");
    stack.push("my_module");
    assert_eq!(stack.fqn_for_symbol("my_fn"), "my_module::my_fn");
    stack.push("MyStruct");
    assert_eq!(
        stack.fqn_for_symbol("method"),
        "my_module::MyStruct::method"
    );
}

#[test]
fn test_fqn_for_anonymous_symbol() {
    let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    stack.push("my_module");
    // Empty symbol name uses parent scope
    assert_eq!(stack.fqn_for_symbol(""), "my_module");
}

#[test]
fn test_dot_separator() {
    let mut stack = ScopeStack::new(ScopeSeparator::Dot);
    stack.push("com");
    stack.push("example");
    stack.push("MyClass");
    assert_eq!(stack.current_fqn(), "com.example.MyClass");
    assert_eq!(
        stack.fqn_for_symbol("myMethod"),
        "com.example.MyClass.myMethod"
    );
}
