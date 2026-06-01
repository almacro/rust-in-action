// 5.9: Isolating and decoding the mantissa from an f32

fn main() {
    let n: f32 = 42.42;
    let n_bits: u32 = n.to_bits();
    let mut mantissa: f32 = 1.0;

    for i in 0..23 {
        let mask = 1 << i;
        let one_at_bit_i = n_bits & mask;
        if one_at_bit_i != 0 {
            let i_ = i as f32;
            let weight = 2_f32.powf( i_ - 23.0 );
            mantissa += weight;
        }
    }

    println!("mantissa: {} {:032b}", mantissa, mantissa.to_bits());
}