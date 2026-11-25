# Services

A script declares the services it uses in `type Services`. It can call only the accessors for services it declared.

```rust
use weave_heart::services::{Scene, Time};
use weave_heart::{Script, ScriptContext};

struct Spinner;

impl Script for Spinner {
    type Services = (Time, Scene);

    fn on_update(&mut self, cx: &mut ScriptContext<'_, Self::Services>) {
        let _dt = cx.time().delta();
        let _root = cx.scene().lookup("world");
    }
}
```

Calling a service the script did not declare fails to compile:

```compile_fail
use weave_heart::services::Time;
use weave_heart::{Script, ScriptContext};

struct Greedy;

impl Script for Greedy {
    type Services = (Time,);

    fn on_update(&mut self, cx: &mut ScriptContext<'_, Self::Services>) {
        let _ = cx.input().is_down(weave_heart::services::KeyCode(32));
    }
}
```

Scripts that declare no services use `()`. The object, the command buffer (`commands()`), and the object's own handle need no declaration.
