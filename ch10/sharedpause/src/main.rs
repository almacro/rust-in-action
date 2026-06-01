// Use a variable defined in a parent scope in multiple closures
use std::{thread, time};

fn main() {
    let pause = time::Duration::from_millis(20);
    let handle1 = thread::spawn(move || {
        thread::sleep(pause);
    });
    let handle2 = thread::spawn(move || {
        thread::sleep(pause);
    });

    let _ = handle1.join();
    let _ = handle2.join();
}
