use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use weave_heart::{Listener, ObjectHandle, Runtime, Script, ScriptContext, Stage, StageContext};

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

fn count() {
    ALLOCATIONS.with(|count| count.set(count.get() + 1));
}

// SAFETY: every method forwards to the system allocator unchanged, and only adds a
// thread-local counter increment, which does not allocate because the Cell is const-initialised.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

struct Silent;

impl Listener for Silent {
    fn on_stage(&mut self, _stage: Stage, _object: ObjectHandle, _cx: &mut StageContext<'_>) {}
}

#[derive(Default)]
struct Spinner {
    turns: u64,
}

impl Script for Spinner {
    fn on_update(&mut self, _cx: &mut ScriptContext<'_>) {
        self.turns += 1;
    }
}

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn steady_state_frames_do_not_allocate() {
    let mut rt = Runtime::new();
    rt.register_script("Spinner", Spinner::default).unwrap();
    let mut objects = Vec::new();
    for i in 0..200 {
        let object = rt.spawn(None, &format!("object{i}")).unwrap();
        if i % 2 == 0 {
            rt.attach(object, Spinner::default()).unwrap();
        }
        rt.commands().load(object);
        objects.push(object);
    }
    for _ in 0..3 {
        rt.tick(0.016, &mut Silent);
    }

    let before = allocations();
    for _ in 0..100 {
        rt.tick(0.016, &mut Silent);
    }
    let after = allocations();

    assert_eq!(
        after - before,
        0,
        "steady-state frames allocated {} times",
        after - before
    );
    assert_eq!(rt.script::<Spinner>(objects[0], 0).unwrap().turns, 103);
}
