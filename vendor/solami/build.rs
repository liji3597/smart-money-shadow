fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Vendored patch (smart-money-shadow): the upstream crate compiles protoc
    // from source via protobuf-src (needs cmake + MSVC C++). Here we rely on
    // tonic-prost-build's own resolution instead: the PROTOC env var, or a
    // `protoc` binary on PATH. See README "Prerequisites".

    let proto_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("proto");
    let geyser = proto_dir.join("geyser.proto");
    let storage = proto_dir.join("solana-storage.proto");

    println!("cargo:rerun-if-changed={}", geyser.display());
    println!("cargo:rerun-if-changed={}", storage.display());

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&[&geyser, &storage], &[&proto_dir])?;

    Ok(())
}
