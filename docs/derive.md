# Deriving components and fields

`#[derive(weave_macros::Component)]` works on structs. Its options are:

- `#[component(name = "...")]` on the struct, to choose the stored name.
- `#[component(rename = "...")]` on a field, to choose its stored name.
- `#[component(skip)]` on a field, to keep it out of the stored data. It is set to `Default` when loaded.

Fields may be any type that implements `Field`: `bool`, `i32`, `i64`, `u32`, `f32`, `f64`, `String`, `[f32; 3]`, `[f32; 4]`, `Vec<String>`, `Option<T>`, and types that derive `Field`. A missing optional field loads as `None`.

`#[derive(weave_macros::Field)]` works on unit-variant enums, which are stored as their variant names, and on structs with one field, which are stored as that field.

Generic types cannot be derived:

```compile_fail
#[derive(weave_macros::Component)]
struct Holder<T> {
    value: T,
}
```

Enums cannot be components. Derive `Field` on them and store them in a struct:

```compile_fail
#[derive(weave_macros::Component)]
enum Mode {
    Idle,
    Run,
}
```

## Checked by tests

`crates/weave-heart/tests/derive.rs` covers unit, tuple, and named structs, the struct and field options, optional fields (present, absent, and missing from older files), unit enums and newtypes deriving `Field`, the numeric types, and an out-of-range integer. The compile-fail examples above are the only checks for the error messages.
