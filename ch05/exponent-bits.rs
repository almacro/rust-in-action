// 5.8: Isolating and decoding the exponent from an f32

fn main() {
    let n: f32 = 42.42;
    let n_bits: u32 = n.to_bits();
    let exponent_ = n_bits >> 23;
    let exponent_ = exponent_ & 0xff;
    let exponent = (exponent_ as i32) - 127;

    println!("exponent: {:08b} {}", exponent, exponent);
}