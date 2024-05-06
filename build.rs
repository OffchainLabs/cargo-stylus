use std::env;

fn main() {
    // Check the target platform
    let target = env::var("TARGET").unwrap();
    let is_windows = target.contains("windows");

    // Enable or disable features based on the platform
    if is_windows {
        println!("printing");
        println!("cargo:rustc-cfg=feature=\"tiny-keccak\"");
        println!("cargo:rustc-cfg=feature=\"postgres\"");
        println!("cargo:rustc-cfg=feature=\"getrandom\"");
        println!("cargo:rustc-cfg=feature=\"rand\"");
        println!("cargo:rustc-cfg=feature=\"rlp\"");
        println!("cargo:rustc-cfg=feature=\"serde\"");
        println!("cargo:rustc-cfg=feature=\"arbitrary\"");
        println!("cargo:rustc-cfg=feature=\"k256\"");
        println!("cargo:rustc-cfg=feature=\"allocative\"");
    } else {
        println!("cargo:rustc-cfg=feature=\"native-keccak\"");
    }
}
