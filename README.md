# waitmap

[![waitmap](https://docs.rs/waitmap/badge.svg)](https://docs.rs/waitmap/)
[![version](https://img.shields.io/crates/v/waitmap)](https://crates.io/crates/waitmap/)

Wait Map is an async/await concurrency primitive implemented as a concurrent hashmap. It is built
on top of the [dashmap](https://github.com/xacrimon/dashmap) concurrent hashmap, with an additional "wait" API.

The wait API lets users wait on one task for an entry to be filled by another task. For example:

```rust
let map: WaitMap<String, Value>;

// This will wait until a value is put under the key "Rosa Luxemburg"
if let Some(value) = map.wait("Rosa Luxemburg").await {
    // ...
}
```

It also supports a cancellation API, to cause any task waiting on an entry being filled to stop
waiting (the future evaluating to `None`, just as if they had called `get` and the key was empty):

```rust
// This will cause the other task to stop waiting, it receives a `None` value:
map.cancel("Rosa Luxemburg");
```
