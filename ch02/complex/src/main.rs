use num::complex::Complex;

fn main() {
    let a = Complex { re: 2.1, im: -1.2 };
    let b = Complex::new( 11.1, 22.2 );
    let result = a + b;

    // normal: 𝑖, bold: 𝒊
    println!("{} + {}𝒊", result.re, result.im);
}
