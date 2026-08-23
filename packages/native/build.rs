extern crate napi_build;

fn main() {
    napi_build::setup();

    if matches!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("windows")
    ) {
        // GPUI links these APIs for prompts and jump lists, but GPUIX does not
        // expose either feature. Loading them eagerly prevents Node and Bun
        // from loading the addon when their process activation context or ICU
        // installation does not provide the expected exports.
        println!("cargo:rustc-cdylib-link-arg=/DELAYLOAD:comctl32.dll");
        println!("cargo:rustc-cdylib-link-arg=/DELAYLOAD:icuuc.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
}
