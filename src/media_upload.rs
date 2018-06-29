extern crate reqwest;
extern crate base64;
extern crate json;
extern crate image;

use MediaType;
use crypto;
use message::FileInfo;
use connection::{WhatsappWebConnection, WhatsappWebHandler};
use std::io::Cursor;
use std::thread;
use std::sync::Arc;
use image::GenericImage;
use std::io::Read;

use std::fs;

pub fn generate_thumbnail_and_get_size(image: &[u8]) -> (Vec<u8>, (u32, u32)) {
    let image = image::load_from_memory(image).unwrap();

    let size = (image.height(), image.width());
    let thumbnail = image.thumbnail(200, 200);

    thumbnail.save("tmp.jpg"); //Todo

    let mut thumbnail = Vec::new();
    fs::File::open("tmp.jpg").unwrap().read_to_end(&mut thumbnail);
    //fs::remove_file("tmp.jpg");
    (thumbnail, size)
}


pub fn upload_file<H>(file: Vec<u8>, media_type: MediaType, connection: &WhatsappWebConnection<H>, callback: Box<Fn(Result<FileInfo, ()>) + Send + Sync>)
    where H: WhatsappWebHandler + Send + Sync + 'static {
    let file_hash = crypto::sha256(&file);

    let file = Arc::new(file);
    let file_hash = Arc::new(file_hash);
    let callback = Arc::new(callback);

    connection.request_file_upload(&file_hash.clone(), media_type, Box::new(move |url: Result<&str, ()>| {
        if let Ok(url) = url {
            let url = url.to_string();
            let file = file.clone();
            let file_hash = file_hash.clone();
            let callback = callback.clone();

            thread::spawn(move || {
                let (file_encrypted, media_key) = crypto::encrypt_media_message(media_type, &file);

                let file_encrypted_hash = crypto::sha256(&file_encrypted);
                let form = reqwest::multipart::Form::new()
                    .text("hash", base64::encode(&file_encrypted_hash))
                    .part("file", reqwest::multipart::Part::reader(Cursor::new(file_encrypted))
                        .mime(reqwest::mime::APPLICATION_OCTET_STREAM));

                let file_info = reqwest::Client::new().post(url.as_str())
                    .multipart(form)
                    .send()
                    .and_then(|mut response| response.text())
                    .map_err(|_| ())
                    .and_then(|response| json::parse(&response).map_err(|_| ()))
                    .and_then(|json| json["url"].as_str().map(|url| url.to_string()).ok_or(()))
                    .map(|url| FileInfo {
                        mime: "image/jpeg".to_string(),
                        sha256: file_hash.to_vec(),
                        enc_sha256: file_encrypted_hash,
                        key: media_key,
                        url,
                        size: file.len() //Or encrypted file size ??
                    });
                callback(file_info);
            });
        } else {
            callback(Err(()));
        }
    }))
}