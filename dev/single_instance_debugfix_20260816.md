# Single Instance build error fix (2026-08-16)

## Symptom

`crates/shortcut-windows/src/single_instance.rs` failed to compile at `wait_for_activation()` with:

- `expected expression, found ==`
- `expected (), found WAIT_EVENT`
- `WAIT_OBJECT_0` reported as unused as a secondary warning

## Cause

The tail expression directly appended `== WAIT_OBJECT_0` to an `unsafe { WaitForSingleObject(...) }` block. In this position Rust parsed the unsafe block as a statement instead of the left-hand side of the comparison.

With `windows` crate 0.62.2, `WaitForSingleObject` returns `WAIT_EVENT`, and `WAIT_OBJECT_0` is also a `WAIT_EVENT`, so no numeric cast is required.

## Fix

```rust
pub fn wait_for_activation(event_value: isize, timeout_ms: u32) -> bool {
    let event = HANDLE(event_value as *mut c_void);
    let wait_result = unsafe { WaitForSingleObject(event, timeout_ms) };
    wait_result == WAIT_OBJECT_0
}
```

A release-script regression gate was also added to reject the old expression form if it is reintroduced.
