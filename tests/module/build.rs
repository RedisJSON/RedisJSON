extern crate bindgen;
extern crate cc;

use std::env;
use std::path::PathBuf;

fn main() {
    const EXPERIMENTAL_API: &str = "REDISMODULE_EXPERIMENTAL_API";

    cc::Build::new()
        .define(EXPERIMENTAL_API, None)
        .file("src/rejson_api.c")
        .include("../../src/include/")
        .compile("rejson_api");

    let bindings = bindgen::Builder::default()
        .clang_arg(format!("-D{}", EXPERIMENTAL_API).as_str())
        .header("../../src/include/rejson_api.h")
        // .allowlist_var("(REDIS|Redis).*")
        .blocklist_type("__darwin_.*")
        .blocklist_type("RedisModule.*")
        .allowlist_type("(RedisJSON|JSON).*")
        // .parse_callbacks(Box::new(RedisModuleCallback))
        .size_t_is_usize(true)
        .generate()
        .expect("error generating bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write bindings to file");
}
