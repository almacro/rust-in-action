fn noop() {}

fn main() {
    let fn_ptr = noop as usize;

    println!("noop as usize: 0x{:x}", fn_ptr)
}