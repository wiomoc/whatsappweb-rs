extern crate base64;
extern crate json;
extern crate image;

use std::io::Cursor;
use std::thread;
use std::sync::Arc;

use json_protocol::JsonNonNull;
use image::{GenericImage, RGB};
use image::jpeg::JPEGEncoder;
use reqwest;

use MediaType;
use crypto;
use message::FileInfo;
use connection::{WhatsappWebConnection, WhatsappWebHandler};
use errors::*;


pub fn generate_thumbnail_and_get_size(image: &[u8]) -> (Vec<u8>, (u32, u32)) {
    let image = image::load_from_memory(image).unwrap();

    let size = (image.height(), image.width());
    let thumbnail = image.thumbnail(160, 160).to_rgb();

    let mut thumbnail_writter = Cursor::new(Vec::new());

    JPEGEncoder::new(&mut thumbnail_writter).encode(&thumbnail, thumbnail.width(), thumbnail.height(), RGB(8)).unwrap();

    (thumbnail_writter.into_inner(), size)
}

/// Download file from servers and decrypt it
pub fn download_file(file_info: FileInfo, media_type: MediaType, callback: Box<Fn(Result<Vec<u8>>) + Send + Sync>) {
    thread::spawn(move || {
        let mut file_enc = Cursor::new(Vec::with_capacity(file_info.size));

        callback(reqwest::get(&file_info.url)
            .map_err(|e| Error::with_chain(e, "could not load file"))
            .and_then(|mut response| {
                let status = response.status();
                if status.is_success() {
                    response.copy_to(&mut file_enc)
                        .map_err(|e| Error::with_chain(e, "could not load file"))
                } else {
                    bail!{"received http status code {}", status.as_u16()}
                }
            })
            .and_then(|_| crypto::decrypt_media_message(&file_info.key, media_type, &file_enc.into_inner())));
    });
}

/// Upload file to servers and encrypt it
pub fn upload_file<H>(file: &[u8], media_type: MediaType, connection: &WhatsappWebConnection<H>, callback: Box<Fn(Result<FileInfo>) + Send + Sync>)
    where H: WhatsappWebHandler + Send + Sync + 'static {
    let file_hash = crypto::sha256(file);

    let file_hash = Arc::new(file_hash);
    let callback = Arc::new(callback);

    let (file_encrypted, media_key) = crypto::encrypt_media_message(media_type, file);
    let file_encrypted_hash = crypto::sha256(&file_encrypted);


    //Todo refactoring, remove arc -> request_file_upload fnonce
    let file_encrypted_hash = Arc::new(file_encrypted_hash);
    let file_encrypted = Arc::new(file_encrypted);
    let media_key = Arc::new(media_key);
    let file_len = file.len();

    connection.request_file_upload(&file_hash.clone(), media_type, Box::new(move |url: Result<&str>| {
        match url {
            Ok(url) => {
                let url = url.to_string();
                let file_hash = file_hash.clone();
                let file_encrypted_hash = file_encrypted_hash.clone();
                let file_encrypted = file_encrypted.clone();
                let media_key = media_key.clone();
                let callback = callback.clone();

                thread::spawn(move || {
                    let form = reqwest::multipart::Form::new()
                        .text("hash", base64::encode(&file_encrypted_hash.to_vec()))
                        .part("file", reqwest::multipart::Part::reader(Cursor::new(file_encrypted.to_vec()))
                            .mime(reqwest::mime::APPLICATION_OCTET_STREAM));

                    let file_info = reqwest::Client::new().post(url.as_str())
                        .multipart(form)
                        .send()
                        .and_then(|mut response| response.text())
                        .map_err(|e| Error::with_chain(e, "could not upload file"))
                        .and_then(|response| json::parse(&response).map_err(|e| (Error::with_chain(e, "invalid response"))))
                        .and_then(|json| json.get_str("url").map(|url| url.to_string()))
                        .map(|url| FileInfo {
                            mime: "image/jpeg".to_string(),
                            sha256: file_hash.to_vec(),
                            enc_sha256: file_encrypted_hash.to_vec(),
                            key: media_key.to_vec(),
                            url,
                            size: file_len, //Or encrypted file size ??
                        });
                    callback(file_info);
                });
            }
            Err(err) => callback(Err(err).chain_err(|| "could not request file upload"))
        }
    }))
}