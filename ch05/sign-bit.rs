// 5.7: Isolating and decoding the sign bit from an f32

fn main() {
  let n: f32 = 42.42;
  let n_bits: u32 = n.to_bits();
  let sign_bit = n_bits >> 31;

  println!("{}\n{:032b}\n{}", n, n_bits, sign_bit);
}