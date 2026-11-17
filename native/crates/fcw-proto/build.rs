use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let proto_dir = manifest.join("../../proto");
    let files = [proto_dir.join("control.proto"), proto_dir.join("stream.proto")];
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    // protoc из крейта protoc-bin-vendored: сборка не требует установленного protoc.
    // PROTOC из окружения (если задан) имеет приоритет.
    if std::env::var_os("PROTOC").is_none() {
        let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
        std::env::set_var("PROTOC", protoc);
    }
    let mut cfg = prost_build::Config::new();
    cfg.compile_protos(&files, &[proto_dir]).expect("compile protos");
}
