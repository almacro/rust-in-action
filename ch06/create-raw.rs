// Creating a raw pointer (*const T)

fn main() {
    let a: i64 = 42;
    let a_ptr = &a as *const i64;

    println!("a: {} ({:p})", a, a_ptr);
}