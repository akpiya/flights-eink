fn main() {
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

