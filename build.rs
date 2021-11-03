extern crate protobuf_codegen_pure;

fn main() {
    protobuf_codegen_pure::Codegen::new()
        .out_dir("src")
        .inputs(&["proto/message_wire.proto"])
        .include("proto")
        .run()
        .expect("protoc");
}