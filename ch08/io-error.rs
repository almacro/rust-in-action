// Rust program that always produces an I/O error
use std::fs::File;

fn main() -> Result<(), std::io::Error> {
    let _f = File::open("invisible.txt")?;

    Ok(())
}
