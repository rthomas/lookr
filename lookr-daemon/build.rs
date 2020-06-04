fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/rpc.proto")?;
    tonic_build::compile_protos("proto/secret.proto")?;
    Ok(())
}
