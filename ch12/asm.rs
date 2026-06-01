// the feature `asm` has been stable since 1.59.0 and no longer requires an attribute to enable
//#![feature(asm)]

//use std::asm;
use std::arch::asm;

fn main() {
    unsafe {
        asm!("int 42");
    }
}