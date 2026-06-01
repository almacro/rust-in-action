// Attempt to share a variable in multiple subthreads

/*
let handle = thread.spawn(|| {
    let start = time::Instant::now();
    // This variable doesn't need to be created in each thread.
    let pause = time::Duration::from_millis(20);
    while start.elapsed() < pause {
        thread::yield_now();
    }
});
*/

use std::{thread, time};

fn main() {
    let pause = time::Duration::from_millis(20);
    let handle1 = thread::spawn(|| {
        thread::sleep(pause);
    });
    let handle2 = thread::spawn(|| {
        thread::sleep(pause);
    });

    handle1.join();
    handle2.join();
}
