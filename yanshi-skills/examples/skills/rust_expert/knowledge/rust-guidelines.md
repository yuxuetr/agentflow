# Rust Coding Guidelines

## Error Handling
- Never use `unwrap()` or `expect()` in production code — always propagate errors with `?`
- Use `thiserror` for library errors, `anyhow` for application errors
- Provide meaningful context with `.with_context(|| ...)`

## Memory Safety
- Prefer `Arc<T>` over raw pointers for shared ownership
- Use `Mutex<T>` or `RwLock<T>` for interior mutability in concurrent code
- Avoid `unsafe` unless absolutely necessary; document every `unsafe` block

## Async Code
- Use `tokio::spawn` for independent tasks, not blocking operations
- Never call blocking I/O inside an async context — use `tokio::task::spawn_blocking`
- Prefer `Arc<Mutex<T>>` over `Rc<RefCell<T>>` in async contexts

## Performance
- Avoid unnecessary `.clone()` — pass references where possible
- Use `Cow<str>` when a function may or may not need to own the string
- Prefer iterators over manual loops for zero-cost transformations
- Use `#[inline]` on hot path functions that are small

## Idiomatic Patterns
- Use the builder pattern for complex struct construction
- Prefer `impl Trait` return types in APIs to hide implementation details
- Use `From`/`Into` for type conversions instead of custom `new` constructors
- Leverage `Default` trait implementations wherever sensible
