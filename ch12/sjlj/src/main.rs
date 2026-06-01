//#![feature(link_llvm_intrinsics)]
#![allow(non_camel_case_types)]
#![cfg(not(windows))]

use libc::{
    SIGALRM, SIGHUP, SIGQUIT, SIGTERM, SIGUSR1,
    c_int, c_void,
};
//use libc::{sigjmp_buf, sigsetjmp, siglongjmp};

use std::mem;

const JMP_BUF_BYTES: usize = 512;
//    mem::size_of::<usize>() * 8;
//type jmp_buf = [i8; JMP_BUF_WIDTH];
type jmp_buf = [u8; JMP_BUF_BYTES];

static mut SHUT_DOWN: bool = false;
//static mut RETURN_HERE: jmp_buf = [0; JMP_BUF_WIDTH];
static mut RETURN_HERE: jmp_buf = [0; JMP_BUF_BYTES];
const MOCK_SIGNAL_AT: usize = 3;

/*
unsafe extern "C" {
    #[link_name = "llvm.eh.sjlj.setjmp"]
    pub fn setjmp(_: *mut i8) -> i32;

    #[link_name = "llvm.eh.sjlj.longjmp"]
    pub fn longjmp(_: *mut i8);
}

unsafe extern "C" {
    fn sigsetjmp(env: *mut c_void, savesigs: c_int) -> c_int;
    fn siglongjmp(env: *mut c_void, val: c_int) -> !;
}
*/
unsafe extern "C" {
    #[link_name = "__sigsetjmp"]
    fn sigsetjmp(env: *mut c_void, savesigs: c_int) -> c_int;
    fn siglongjmp(env: *mut c_void, val: c_int) -> !;
}

#[inline]
fn ptr_to_jmp_buf() -> *mut i8 {
    //unsafe { &raw const RETURN_HERE as *const i8 as *mut i8 }
    &raw const RETURN_HERE as *const i8 as *mut i8
}

#[inline]
fn return_early() {
    let franken_pointer = ptr_to_jmp_buf();
    unsafe { siglongjmp(franken_pointer as *mut c_void, 1) };
}

fn register_signal_handler() {
    unsafe {
        libc::signal(SIGUSR1, handle_signals as *const () as usize);
    }
}

#[allow(dead_code)]
fn handle_signals(sig: i32) {
    register_signal_handler();

    let should_shut_down = match sig {
        SIGHUP => false,
        SIGALRM => false,
        SIGTERM => true,
        SIGQUIT => true,
        SIGUSR1 => true,
        _ => false,
    };

    unsafe {
        SHUT_DOWN = should_shut_down;
    }

    return_early();
}

fn print_depth(depth: usize) {
    for _ in 0..depth {
        print!("#");
    }
    println!();
}

fn dive(depth: usize, max_depth: usize) {
    unsafe {
        if SHUT_DOWN {
            println!("!");
            return;
        }
    }
    print_depth(depth);

    if depth >= max_depth {
        return
    } else if depth == MOCK_SIGNAL_AT {
        unsafe {
            libc::raise(SIGUSR1);
        }
    } else {
        dive(depth + 1, max_depth);
    }
    print_depth(depth);
}
fn main() {
    const JUMP_SET: i32 = 0;

    register_signal_handler();

    let return_point = ptr_to_jmp_buf();
    let rc = unsafe { sigsetjmp(return_point as *mut c_void, 1) };
    if rc == JUMP_SET {
        dive(0, 10);
    } else {
        println!("early return");
    }

    println!("finishing!");
}
