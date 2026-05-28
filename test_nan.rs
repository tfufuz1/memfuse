fn main() {
    println!("max(0.01, NaN) = {}", f32::max(0.01, f32::NAN));
    println!("max(NaN, 0.01) = {}", f32::max(f32::NAN, 0.01));
}
