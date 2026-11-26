# Services

A script that uses no services needs no declaration:

```rust
use weave_heart::{Script, ScriptContext};

struct Blink;

impl Script for Blink {
    fn on_update(&mut self, cx: &mut ScriptContext<'_>) {
        cx.set_enabled(true);
    }
}
```

A script that uses services declares them in its `impl` header, as the type parameter of `Script`. A service is available only when it is declared:

```rust
use weave_heart::services::{Scene, Time};
use weave_heart::{Script, ScriptContext};

struct Spinner;

impl Script<(Time, Scene)> for Spinner {
    fn on_update(&mut self, cx: &mut ScriptContext<'_, (Time, Scene)>) {
        let _dt = cx.time().delta();
        let _root = cx.scene().lookup("world");
    }
}
```

Calling a service the script did not declare fails to compile. This script declares only `Time`, and asks for input:

```compile_fail
use weave_heart::services::Time;
use weave_heart::{Script, ScriptContext};

struct Greedy;

impl Script<(Time,)> for Greedy {
    fn on_update(&mut self, cx: &mut ScriptContext<'_, (Time,)>) {
        let _ = cx.input().is_down(weave_heart::services::KeyCode(32));
    }
}
```

A script with no declaration cannot use services either:

```compile_fail
use weave_heart::{Script, ScriptContext};

struct Lazy;

impl Script for Lazy {
    fn on_update(&mut self, cx: &mut ScriptContext<'_>) {
        let _ = cx.time().delta();
    }
}
```

The object, the command buffer, and the object's own handle need no declaration. The helpers `set_transform`, `set_enabled`, `despawn`, `reparent`, `insert_component`, and `edit_component` work in every script.
