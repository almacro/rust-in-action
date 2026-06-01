⏺ The segfault is raised inside longjmp(franken_pointer) at line 36, called from the signal handler via return_early()
  at line 62.
 
  How it gets there

  main → setjmp (line 98) → fills RETURN_HERE → dive(0, 10) (line 100)
                                                   ↓
                                         recurses until depth==3 (MOCK_SIGNAL_AT)
                                                   ↓
                                    libc::raise(SIGUSR1) (line 85)
                                                   ↓
                                         kernel delivers signal
                                                   ↓
                                    handle_signals(SIGUSR1) (line 46)
                                                   ↓
                                         return_early()  (line 62)
                                                   ↓
                                    longjmp(franken_pointer)  (line 36)  ← segfault here

  Why it segfaults (three compounding issues)

  1. llvm.eh.sjlj.setjmp/longjmp are not libc setjmp/longjmp. These LLVM intrinsics are designed for SJLJ-style
  exception unwinding inside LLVM's EH infrastructure. They depend on landing pad addresses and personality-function
  metadata that the compiler only emits when the function is part of a proper exception scope. Using them as a drop-in
  for setjmp/longjmp is documented as unsupported. longjmp ends up jumping to an effectively-undefined PC → segfault.
  2. JMP_BUF_WIDTH = size_of::<usize>() * 8 = 64 is the wrong size on arm64. The buffer is treated as [i8; 64]. The
  actual state LLVM stores varies by target — on AArch64 it's larger than 64 bytes. So setjmp writes past the end of the
   static, and longjmp reads partial garbage. Even if (1) weren't true, this would corrupt adjacent statics or the read
  path.
  3. longjmp from a signal handler is POSIX UB unless you use sigsetjmp/siglongjmp. The book skips that. On macOS the
  legacy libc::signal() API + non-sig variants is enough to make the runtime state inconsistent (signal mask not
  restored, etc.), which on arm64 often surfaces as a segfault inside the jump itself rather than later.

  The root cause is (1) — the book's choice to use the LLVM intrinsics directly. (2) and (3) make it worse.

⏺ All predictions confirmed. This is the definitive answer:

  Prediction: Fault PC = pop %rbp at return_early+33
  Actual: rip = 0x...82e1, instruction pop %rbp  
  ✓: ✓
  ────────────────────────────────────────       
  Prediction: Fault VA = 0x10
  Actual: si_addr = 0x10                         
  ✓: ✓
  ────────────────────────────────────────
  Prediction: %rsp corrupted to 0 then incremented to 0x10
  Actual: rsp = 0x10 
  ✓: ✓
  ────────────────────────────────────────
  Prediction: %rbp restored to garbage (zero) from buf[0]
  Actual: rbp = 0x0
  ✓: ✓
  ────────────────────────────────────────
  Prediction: %rax holds the landing pad PC (intended jump target)
  Actual: rax = 0x555555568554 ← matches the landing pad address from main's disassembly
  ✓: ✓
  ────────────────────────────────────────
  Prediction: setjmp only wrote buf[8]; buf[0] and buf[16] stayed zero
  Actual: See below
  ✓: ✓
  
  RETURN_HERE contents at the moment of crash:

  buf[0..8]   = 00 00 00 00 00 00 00 00            ← FP slot, never written
  buf[8..16]  = 54 85 56 55 55 55 00 00            ← PC slot = 0x555555568554 (landing pad)
  buf[16..24] = 00 00 00 00 00 00 00 00            ← SP slot, never written

  Exactly what we predicted: a buffer with only the middle 8 bytes populated, surrounded by zeros on both sides.

  Backtrace confirms the fault is the very first stack access after the bad register restore — gdb can only show
  return_early itself and then bails out (Cannot access memory at address 0x18) because it tried to follow %rbp = 0 to
  the next frame and there isn't one.
  
  One-line summary

  llvm.eh.sjlj.setjmp writes the landing-pad PC into buf[8] and nothing else, but llvm.eh.sjlj.longjmp restores %rbp,
  %rax, and %rsp from buf[0], buf[8], buf[16] — so %rsp ends up at 0 (from the still-zero buf[16]), and the function
  epilogue's pop %rbp immediately faults trying to read from address 0x10. The intrinsics are LLVM-internal helpers for
  SJLJ exception handling and rely on additional codegen that the frontend emits inside proper EH scopes; the book uses
  them outside any such scope, so the missing code never gets emitted and the buffer is half-populated.

❯ how to set proper EH scope in the right places in this code?                                                          

⏺ Short answer: you can't easily set proper EH scope in pure Rust — and even if you fill in the missing buffer slots by
  hand, the result isn't reliable. Let me explain what "proper EH scope" actually requires, then give you the three
  practical paths.

  What proper LLVM SJLJ EH scope needs
  
  For llvm.eh.sjlj.setjmp/longjmp to work, the LLVM frontend (clang in C/C++, rustc in Rust) has to emit a lot of
  supporting infrastructure that the intrinsics themselves don't generate:

  1. A personality function attached to the calling function via attribute — typically __gcc_personality_sj0 (the SJLJ
  personality). Tells the unwinder which model is in use.
  2. invoke instructions instead of call at every possible throw site, with a landingpad block as the unwind target.
  This is how the frontend wires up "where to land" — it's structured at the IR level, not a runtime decision.
  3. Frontend-emitted FP/SP stores at the setjmp callsite — i.e., buf[0] = __builtin_frame_address(0) and buf[16] = 
  __builtin_stack_save(). Clang's __builtin_setjmp(buf) emits these in addition to the bare llvm.eh.sjlj.setjmp(buf)
  intrinsic. The intrinsic alone only writes buf[8].

  Rust exposes none of this in stable or easily-usable form:

  - No way to attach a custom personality function from user code (Rust has its own rust_eh_personality for
  panic=unwind, and you can't easily swap it).
  - No way to emit invoke IR from Rust source — Rust always emits call. The unwind machinery uses invoke only for the
  panic path, baked into the compiler.
  - No stable __builtin_frame_address / __builtin_stack_save equivalent.

  So this isn't a matter of "add an attribute here" — the book's example is using LLVM-internal intrinsics that
  fundamentally require frontend cooperation Rust doesn't provide.

  Three practical paths (most → least faithful to the book's spirit)

  Path 1: Drop the intrinsics, use libc::setjmp/longjmp (recommended)
  
  The most pragmatic fix. The libc functions are real C symbols that correctly save and restore all needed state on
  every platform, and they don't need any EH scope:

  // Replace the unsafe extern "C" block + intrinsics with:
  use libc::{c_int, c_void};

  unsafe extern "C" {
      fn setjmp(env: *mut c_void) -> c_int;
      fn longjmp(env: *mut c_void, val: c_int) -> !;
  }

  // And size the buffer generously — libc's jmp_buf is much larger than
  // 64 bytes on real platforms (often 200+ on Linux glibc, ~150 on macOS).
  const JMP_BUF_BYTES: usize = 512;
  type jmp_buf = [u8; JMP_BUF_BYTES];
  static mut RETURN_HERE: jmp_buf = [0; JMP_BUF_BYTES];

  Then call setjmp(ptr_to_jmp_buf() as *mut c_void) and longjmp(ptr, 1). Everything else stays the same.

  Caveat: longjmp from inside a signal handler is technically POSIX UB unless you use sigsetjmp/siglongjmp. For
  correctness with the signal-handler flow:

  unsafe extern "C" {
      fn sigsetjmp(env: *mut c_void, savesigs: c_int) -> c_int;
      fn siglongjmp(env: *mut c_void, val: c_int) -> !;
  }
  // then sigsetjmp(buf, 1) and siglongjmp(buf, 1)

  This preserves the signal mask, which longjmp does not.

  Path 2: Write a C shim with __builtin_setjmp, link from Rust

  If the book really wants to demonstrate the LLVM intrinsics, get them properly via a C compiler that will emit the
  FP/SP stores and personality metadata correctly:

  // sjlj_shim.c
  #include <stdint.h>

  int my_setjmp(void *buf) {
      return __builtin_setjmp(buf);
  }

  void my_longjmp(void *buf) {
      __builtin_longjmp(buf, 1);
  }
  
  // build.rs
  fn main() {
      cc::Build::new().file("sjlj_shim.c").compile("sjlj_shim");
  }

  // main.rs
  unsafe extern "C" {
      fn my_setjmp(buf: *mut i8) -> i32;
      fn my_longjmp(buf: *mut i8);
  }

  __builtin_setjmp in Clang expands to the full setjmp idiom (FP store + SP store + intrinsic), and __builtin_longjmp is
   the matched pair. Buffer size required for __builtin_setjmp is documented as 5 words (40 bytes on 64-bit) — your
  64-byte buffer is fine.
  
  Path 3: Stay with the intrinsics, fill buf[0] and buf[16] manually via inline asm

  The closest "fix in place" — patches the missing stores by capturing FP/SP from inline asm right before calling
  setjmp. Sketch (x86_64):

  #[inline(always)]
  unsafe fn setjmp_filled(buf: *mut i8) -> i32 {
      let fp: usize;
      let sp: usize;
      core::arch::asm!(
          "mov {fp}, rbp",
          "mov {sp}, rsp",
          fp = out(reg) fp,
          sp = out(reg) sp,
          options(nomem, nostack, preserves_flags),
      );
      let buf_words = buf as *mut usize;
      buf_words.add(0).write(fp);            // buf[0]  = FP
      buf_words.add(2).write(sp);            // buf[16] = SP
      setjmp(buf)                            // intrinsic writes buf[8] = landing PC
  }

  This unblocks the immediate segfault — pop %rbp will no longer fault, because %rsp gets restored to a valid stack
  address — but the resumed execution still runs against a partially-restored frame. Without an invoke/landingpad to
  tell LLVM that the resumption point invalidates the in-flight register allocation, the landing pad may read live
  values from registers/stack slots that have since been clobbered. You can easily get incorrect output, weird state, or
   later crashes. It's "no longer segfaults immediately" rather than "works correctly."

  You'd also need to repeat this dance for AArch64 (fp/sp register names different), keep #[inline(always)] to ensure
  FP/SP captured are the caller's, and accept that the SP value at the moment of capture might still be slightly off
  (compiler-inserted stack adjustments).
  
  Recommendation

  For a learning exercise, Path 1 with sigsetjmp/siglongjmp is the most honest path: it teaches non-local control flow
  exactly as POSIX intends, works on every platform, and survives a signal handler correctly. You'd change six lines of
  code and the program does what the book intended.
  
  If you specifically want to see LLVM intrinsics in action with the right scaffolding, Path 2 is the right way — Clang
  generates the supporting IR, the intrinsics behave correctly, and you observe the SJLJ EH machinery as designed.

  Path 3 is interesting for showing what's missing, but I wouldn't trust it for anything beyond a "look, no segfault"
  demo.

⏺ Glibc quirk. sigsetjmp isn't actually a function on Linux — it's a macro in <setjmp.h> that expands to
  __sigsetjmp(env, savemask). The real exported symbol in libc.so is __sigsetjmp, not sigsetjmp. Rust's FFI can't see C
  macros, so when you declare extern { fn sigsetjmp(…); }, the linker goes looking for a symbol that doesn't exist.

  siglongjmp is a real symbol — no issue there. Same for plain setjmp and longjmp. It's specifically sigsetjmp that's
  affected on glibc.

  Two fixes

  Fix A — Use the libc crate's wrapper (already in your deps)
  
  The libc crate handles the symbol-name remapping for you:

  use libc::{sigjmp_buf, sigsetjmp, siglongjmp};

  static mut RETURN_HERE: sigjmp_buf = unsafe { core::mem::zeroed() };

  // at setjmp site:
  let rc = unsafe { sigsetjmp(&raw mut RETURN_HERE, 1) };

  // at longjmp site:
  unsafe { siglongjmp(&raw mut RETURN_HERE, 1) };
  
  sigjmp_buf from the libc crate is the right size for your platform — drop your hand-rolled jmp_buf typedef.

  Fix B — Keep the manual extern block, but use the right link name

  If you want to stay closer to the book's pedagogical style (declaring extern "C" symbols yourself):

  use libc::{c_int, c_void};
  
  unsafe extern "C" {
      #[link_name = "__sigsetjmp"]
      fn sigsetjmp(env: *mut c_void, savesigs: c_int) -> c_int;
      fn siglongjmp(env: *mut c_void, val: c_int) -> !;
  }

  The #[link_name = "__sigsetjmp"] attribute tells rustc the real symbol to look up; the Rust-side name sigsetjmp stays
  familiar.

  Buffer size

  If you took the byte-array approach, bump it. sigjmp_buf on glibc x86_64 is ~200 bytes (the __jmp_buf_tag struct holds
   8 saved registers + a sigset_t + flags). Your 64-byte buffer is fine for compatibility on most platforms in practice,
   but if you want to be safe:
  
  type jmp_buf = [u8; 512];           // generous, fits any platform
  static mut RETURN_HERE: jmp_buf = [0; 512];

  Or just use libc::sigjmp_buf (Fix A) and let the type system pick the right size.

  Cross-platform note
  
  On macOS (and a few BSDs), sigsetjmp is a real symbol — no macro. So extern { fn sigsetjmp(…); } links on macOS but
  fails on glibc. The portable answer is either Fix A (let libc handle it) or #[link_name = "__sigsetjmp"] gated
  #[cfg(target_env = "gnu")] if you want the manual route to work on both.


⏺ `&raw mut return_point` takes the address of the local variable holding the buffer pointer — giving you `*mut *mut i8`.
  You want the buffer pointer itself, not a pointer to it. Just cast return_point:
```
  let rc = unsafe { sigsetjmp(return_point as *mut c_void, 1) };
```

  Or eliminate the intermediate variable:
```
  let rc = unsafe { sigsetjmp(ptr_to_jmp_buf() as *mut c_void, 1) };
```

  Same shape applies inside return_early:
```
  fn return_early() {
      let franken_pointer = ptr_to_jmp_buf();
      unsafe { siglongjmp(franken_pointer as *mut c_void, 1) };
  }
```

  The &raw mut RETURN_HERE syntax from my earlier example only works if you also changed RETURN_HERE to be typed as
  libc::sigjmp_buf directly (static mut RETURN_HERE: sigjmp_buf = unsafe { core::mem::zeroed() }). You kept the
  byte-array type and the existing ptr_to_jmp_buf() helper, which is fine — just cast its return value to *mut c_void at
   each call site.

⏺ The libc crate (at least the 0.2.186 you have) doesn't expose sigjmp_buf/sigsetjmp/siglongjmp as Rust items — they're
  notoriously absent because of the glibc macro situation and the complex buffer type. Fix A was wrong for your setup;
  you need Fix B.

  Here's a complete, drop-in version that doesn't depend on libc exporting these:
```
  use libc::{c_int, c_void};
  
  unsafe extern "C" {
      #[link_name = "__sigsetjmp"]
      fn sigsetjmp(env: *mut c_void, savesigs: c_int) -> c_int;
      fn siglongjmp(env: *mut c_void, val: c_int) -> !;
  }

  // sigjmp_buf isn't a type the libc crate exposes. Use a generous byte array.
  // glibc x86_64's sigjmp_buf is ~200 bytes; 512 is safely larger than any
  // real platform's sigjmp_buf, so this works portably without per-target sizing.
  const JMP_BUF_BYTES: usize = 512;
  type jmp_buf = [u8; JMP_BUF_BYTES];

  static mut RETURN_HERE: jmp_buf = [0; JMP_BUF_BYTES];
```

  Then your existing `ptr_to_jmp_buf()` helper still works — just cast its `*mut i8` return value to `*mut c_void` at each call site:
```
  // in main:
  let rc = unsafe { sigsetjmp(ptr_to_jmp_buf() as *mut c_void, 1) };
  if rc == JUMP_SET {
      dive(0, 10);
  } else {
      println!("early return");
  }

  // in return_early:
  fn return_early() {
      unsafe { siglongjmp(ptr_to_jmp_buf() as *mut c_void, 1) };
  }
```

  A few things to delete since they were part of the LLVM-intrinsic approach:

  - `#![feature(link_llvm_intrinsics)]` (line 1) — not needed; you can drop nightly too if you want
  - The `setjmp` / `longjmp` declarations with `#[link_name = "llvm.eh.sjlj.…"]` — replaced by the  `sigsetjmp`/`siglongjmp` extern
   block above
  - The old JMP_BUF_WIDTH constant and [i8; JMP_BUF_WIDTH] type — replaced

  Build with stable now:
```    
  cargo build && ./target/debug/sjlj
```

  Expected output:
```
                  ← print_depth(0) blank line
  #               ← dive(1)
  ##              ← dive(2)
  ###             ← dive(3) — raises SIGUSR1, signal handler runs, siglongjmp jumps back
  early return    ← landing back at main's setjmp site with rc != 0
  finishing!
```
  No recursion unwind, no segfault, no second set of "##  #" lines. That's the indicator the jump actually worked.
