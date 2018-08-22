use std::io;
use ws;
use ring;
#[cfg(feature = "media")]
use reqwest;
use json;
use base64;
use protobuf;

error_chain! {
        foreign_links {
            Io(io::Error);
            Websocket(ws::Error);
            Crypto(ring::error::Unspecified);
            Reqwest(reqwest::Error) #[cfg(feature = "media")];
            Json(json::Error);
            Base64(base64::DecodeError);
            Protobuf(protobuf::ProtobufError);
        }

        errors {
            NodeAttributeMissing(attribute: &'static str) {
                description("missing node attribute")
                display("missing mode attribute '{}'", attribute)
            }

            JsonFieldMissing(field: &'static str) {
                description("missing field in json")
                display("missing field '{}' in json", field)
            }
        }
}
