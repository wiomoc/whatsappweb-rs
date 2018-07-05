extern crate crypto;

use ring;
use ring::{agreement, rand, hkdf, hmac, digest};
use ring::rand::{SystemRandom, SecureRandom};
use self::crypto::{aes, blockmodes};
use self::crypto::buffer::{RefWriteBuffer, RefReadBuffer, WriteBuffer};
use untrusted;

use MediaType;
use errors::*;

pub(crate) fn generate_keypair() -> (agreement::EphemeralPrivateKey, Vec<u8>) {
    let rng = rand::SystemRandom::new();

    let my_private_key =
        agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();

    let mut my_public_key = vec![0u8; my_private_key.public_key_len()];
    my_private_key.compute_public_key(&mut my_public_key).unwrap();

    (my_private_key, my_public_key)
}

pub(crate) fn calculate_secret_keys(secret: &[u8], private_key: agreement::EphemeralPrivateKey) -> Result<([u8; 32], [u8; 32])> {
    let peer_public_key_alg = &agreement::X25519;

    let public_key = untrusted::Input::from(&secret[..32]);


    let secret_key = agreement::agree_ephemeral(private_key, peer_public_key_alg,
                                                public_key, ring::error::Unspecified,
                                                |key_material| {
                                                    Ok(Vec::from(key_material))
                                                }).unwrap();
    let mut secret_key_expanded = [0u8; 80];

    hkdf::extract_and_expand(&hmac::SigningKey::new(&digest::SHA256, &[0u8; 32]), &secret_key, &[], &mut secret_key_expanded);

    let signature = [&secret[..32], &secret[64..]].concat();

    hmac::verify(&hmac::VerificationKey::new(&digest::SHA256, &secret_key_expanded[32..64]), &signature, &secret[32..64]).chain_err(|| "Invalid mac")?;

    let mut buffer = [0u8; 64];

    aes_decrypt(&secret_key_expanded[..32], &secret_key_expanded[64..], &secret[64..144], &mut buffer);

    let mut enc = [0; 32];
    let mut mac = [0; 32];

    enc.copy_from_slice(&buffer[..32]);
    mac.copy_from_slice(&buffer[32..]);


    Ok((enc, mac))
}

pub fn verify_and_decrypt_message(enc: &[u8], mac: &[u8], message_encrypted: &[u8]) -> Result<Vec<u8>> {
    hmac::verify(&hmac::VerificationKey::new(&digest::SHA256, &mac),
                 &message_encrypted[32..], &message_encrypted[..32]).chain_err(|| "Invalid mac")?;

    let mut message = vec![0u8; message_encrypted.len() - 48];

    let size_without_padding = aes_decrypt(enc, &message_encrypted[32..48], &message_encrypted[48..], &mut message);
    message.truncate(size_without_padding);
    Ok(message)
}

pub(crate) fn sign_and_encrypt_message(enc: &[u8], mac: &[u8], message: &[u8]) -> Vec<u8> {
    let mut message_encrypted = vec![0u8; 32 + 16 + message.len() + 32];


    let mut iv = vec![0u8; 16];
    SystemRandom::new().fill(&mut iv).unwrap();

    let size_with_padding = aes_encrypt(enc, &iv, &message, &mut message_encrypted[48..]);
    message_encrypted.truncate(32 + 16 + size_with_padding);

    message_encrypted[32..48].clone_from_slice(&iv);

    let signature = hmac::sign(&hmac::SigningKey::new(&digest::SHA256, &mac),
                               &message_encrypted[32..]);

    message_encrypted[0..32].clone_from_slice(signature.as_ref());
    message_encrypted
}

pub(crate) fn sign_challenge(mac: &[u8], challenge: &[u8]) -> hmac::Signature {
    hmac::sign(&hmac::SigningKey::new(&digest::SHA256, &mac), &challenge)
}

fn derive_media_keys(key: &[u8], media_type: MediaType) -> [u8; 112] {
    let mut media_key_expanded = [0u8; 112];
    hkdf::extract_and_expand(&hmac::SigningKey::new(&digest::SHA256, &[0u8; 32]), key, match media_type {
        MediaType::Image => b"WhatsApp Image Keys",
        MediaType::Video => b"WhatsApp Video Keys",
        MediaType::Audio => b"WhatsApp Audio Keys",
        MediaType::Document => b"WhatsApp Document Keys",
    }, &mut media_key_expanded);
    media_key_expanded
}

pub fn sha256(file: &[u8]) -> Vec<u8> {
    let mut hash = Vec::with_capacity(32);
    hash.extend_from_slice(digest::digest(&digest::SHA256, file).as_ref());
    hash
}

pub fn encrypt_media_message(media_type: MediaType, file: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut media_key = vec![0u8; 32];
    SystemRandom::new().fill(&mut media_key).unwrap();
    let media_key_expanded = derive_media_keys(&media_key, media_type);

    let mut file_encrypted = vec![0u8; 10 + file.len() + 32];


    let mut cipher_key = Vec::with_capacity(32);
    cipher_key.extend_from_slice(&media_key_expanded[16..48]);

    let iv = &media_key_expanded[0..16];

    let size_with_padding = aes_encrypt(&cipher_key, iv, &file, &mut file_encrypted);
    file_encrypted.truncate(size_with_padding);

    let hmac_data = [iv, &file_encrypted].concat();

    let signature = hmac::sign(&hmac::SigningKey::new(&digest::SHA256, &media_key_expanded[48..80]),
                               &hmac_data);

    file_encrypted.extend_from_slice(&signature.as_ref()[0..10]);
    (file_encrypted, media_key)
}

pub fn decrypt_media_message(key: &[u8], media_type: MediaType, file_encrypted: &[u8]) -> Result<Vec<u8>> {
    let media_key_expanded = derive_media_keys(key, media_type);

    let mut file = vec![0u8; file_encrypted.len() - 10];

    let mut cipher_key = Vec::with_capacity(32);
    cipher_key.extend_from_slice(&media_key_expanded[16..48]);

    let size = file_encrypted.len();

    let hmac_data = [&media_key_expanded[0..16], &file_encrypted[..size - 10]].concat();

    let signature = hmac::sign(&hmac::SigningKey::new(&digest::SHA256, &media_key_expanded[48..80]),
                               &hmac_data);

    if file_encrypted[(size - 10)..] != signature.as_ref()[..10] {
        bail! {"Invalid mac"}
    }


    let size_without_padding = aes_decrypt(&cipher_key, &media_key_expanded[0..16], &file_encrypted[..size - 10], &mut file);
    file.truncate(size_without_padding);

    Ok(file)
}

pub(crate) fn aes_encrypt(key: &[u8], iv: &[u8], input: &[u8], output: &mut [u8]) -> usize {
    let mut aes_encrypt = aes::cbc_encryptor(aes::KeySize::KeySize256, key, iv, blockmodes::PkcsPadding);

    let mut read_buffer = RefReadBuffer::new(input);

    let mut write_buffer = RefWriteBuffer::new(output);

    aes_encrypt.encrypt(&mut read_buffer, &mut write_buffer, true).unwrap();
    write_buffer.position()
}

pub(crate) fn aes_decrypt(key: &[u8], iv: &[u8], input: &[u8], output: &mut [u8]) -> usize {
    let mut aes_decrypt = aes::cbc_decryptor(aes::KeySize::KeySize256, key, iv, blockmodes::PkcsPadding);

    let mut read_buffer = RefReadBuffer::new(input);

    let mut write_buffer = RefWriteBuffer::new(output);

    aes_decrypt.decrypt(&mut read_buffer, &mut write_buffer, true).unwrap();
    write_buffer.position()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64;
    use node_wire::Node;
    use std::io::stdin;


    #[test]
    #[ignore]
    fn decrypt_node_from_browser() {
        let enc = base64::decode("").unwrap();

        let mac = base64::decode("").unwrap();

        loop {
            let mut line = String::new();
            stdin().read_line(&mut line).unwrap();
            let len = line.len();
            line.truncate(len - 1);
            let msg = base64::decode(&line).unwrap();
            let pos = msg.iter().position(|x| x == &b',').unwrap() + 3;

            let dec_msg = verify_and_decrypt_message(&enc, &mac, &msg[pos..]).unwrap();

            let node = Node::deserialize(&dec_msg).unwrap();

            println!("{:?}", node);
        }
    }

    #[test]
    fn test_encrypt_decrypt_message() {
        let mut enc = vec![0u8; 32];
        SystemRandom::new().fill(&mut enc).unwrap();

        let mut mac = vec![0u8; 32];
        SystemRandom::new().fill(&mut mac).unwrap();

        let mut msg = vec![0u8; 30];
        SystemRandom::new().fill(&mut msg).unwrap();
        let enc_msg = sign_and_encrypt_message(&enc, &mac, &msg);

        let dec_msg = verify_and_decrypt_message(&enc, &mac, &enc_msg).unwrap();

        assert_eq!(msg, dec_msg);
    }

    #[test]
    fn test_encrypt_decrypt_media() {
        let mut msg = vec![0u8; 300];
        SystemRandom::new().fill(&mut msg).unwrap();

        let media_type = MediaType::Image;

        let (enc_msg, key) = encrypt_media_message(media_type, &msg);

        let dec_msg = decrypt_media_message(&key, media_type, &enc_msg).unwrap();

        assert_eq!(msg, dec_msg);
    }
}
