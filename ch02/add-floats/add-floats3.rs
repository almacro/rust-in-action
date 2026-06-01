// NaNs are never equal
fn main() {
  let x = (-42.0_f32).sqrt();
  assert_eq!(x, x);
}
