// Program defensively by using is_nan() and is_finite()
fn main() {
  let x: f32 = 1.0 / 0.0;
  assert!(x.is_finite());
}
