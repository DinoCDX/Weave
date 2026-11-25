# weave-macros

Owner area: core. Boundary: derive macros for WeaveHeart. Depends on no project crate.

`#[derive(Component)]` works on structs with named fields and no generics. It implements `weave_heart::Component` and generates the component's name, its schema hash, its field schema, and conversions to and from field values.
