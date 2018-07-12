use std::time::Duration;
use std::str::FromStr;

use protobuf;
use chrono::NaiveDateTime;
use protobuf::Message;
use ring::rand::{SystemRandom, SecureRandom};

use super::message_wire;
use super::Jid;
use errors::*;

#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn generate() -> MessageId {
        let mut message_id_binary = vec![0u8; 12];
        message_id_binary[0] = 0x3E;
        message_id_binary[1] = 0xB0;
        SystemRandom::new().fill(&mut message_id_binary[2..]).unwrap();
        MessageId(message_id_binary.iter().map(|b| format!("{:X}", b)).collect::<Vec<_>>().concat())
    }
}


#[derive(Debug, Clone)]
pub enum Peer {
    Individual(Jid),
    Group { group: Jid, participant: Jid },
}

#[derive(Debug, Clone)]
pub enum PeerAck {
    Individual(Jid),
    GroupIndividual { group: Jid, participant: Jid },
    GroupAll(Jid),
}

#[derive(Debug)]
pub enum Direction {
    Sending(Jid),
    Receiving(Peer),
}

impl Direction {
    fn parse(mut key: message_wire::MessageKey) -> Result<Direction> {
        let remote_jid = Jid::from_str(&key.take_remoteJid())?;
        Ok(if key.get_fromMe() {
            Direction::Sending(remote_jid)
        } else {
            Direction::Receiving(if key.has_participant() {
                Peer::Group { group: remote_jid, participant: Jid::from_str(&key.take_participant())? }
            } else {
                Peer::Individual(remote_jid)
            })
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub enum MessageAckLevel {
    PendingSend = 0,
    Send = 1,
    Received = 2,
    Read = 3,
    Played = 4,
}

#[derive(Debug)]
pub enum MessageAckSide {
    Here(Peer),
    There(PeerAck),
}

#[derive(Debug)]
pub struct MessageAck {
    pub level: MessageAckLevel,
    pub time: Option<i64>,
    pub id: MessageId,
    pub side: MessageAckSide,
}

impl MessageAck {
    pub fn from_server_message(message_id: &str, level: MessageAckLevel, sender: Jid, receiver: Jid, participant: Option<Jid>, time: i64, own_jid: &Jid) -> MessageAck {
        MessageAck {
            level,
            time: Some(time),
            id: MessageId(message_id.to_string()),
            side: if own_jid == &sender {
                MessageAckSide::There(if let Some(participant) = participant {
                    PeerAck::GroupIndividual { group: receiver, participant }
                } else {
                    PeerAck::Individual(receiver)
                })
            } else {
                MessageAckSide::Here(if let Some(participant) = participant {
                    Peer::Group { group: sender, participant }
                } else {
                    Peer::Individual(sender)
                })
            },
        }
    }

    pub fn from_app_message(message_id: MessageId, level: MessageAckLevel, jid: Jid, participant: Option<Jid>, owner: bool) -> MessageAck {
        MessageAck {
            level,
            time: None,
            id: message_id,
            side: if owner {
                MessageAckSide::There(if jid.is_group {
                    PeerAck::GroupAll(jid)
                } else {
                    PeerAck::Individual(jid)
                })
            } else {
                MessageAckSide::Here(if let Some(participant) = participant {
                    Peer::Group { group: jid, participant }
                } else {
                    Peer::Individual(jid)
                })
            },
        }
    }
}

#[derive(Debug)]
pub struct FileInfo {
    pub url: String,
    pub mime: String,
    pub sha256: Vec<u8>,
    pub enc_sha256: Vec<u8>,
    pub size: usize,
    pub key: Vec<u8>,
}

#[derive(Debug)]
pub enum ChatMessageContent {
    Text(String),
    Image(FileInfo, (u32, u32), Vec<u8>),
    Audio(FileInfo, Duration),
    Document(FileInfo, String),
}

impl ChatMessageContent {
    fn from_proto(mut message: message_wire::Message) -> Result<ChatMessageContent> {
        Ok(if message.has_conversation() {
            ChatMessageContent::Text(message.take_conversation())
        } else if message.has_imageMessage() {
            let mut image_message = message.take_imageMessage();
            ChatMessageContent::Image(FileInfo {
                url: image_message.take_url(),
                mime: image_message.take_mimetype(),
                sha256: image_message.take_fileSha256(),
                enc_sha256: image_message.take_fileEncSha256(),
                size: image_message.get_fileLength() as usize,
                key: image_message.take_mediaKey(),
            }, (image_message.get_height(), image_message.get_width()), image_message.take_jpegThumbnail())
        } else if message.has_audioMessage() {
            let mut audio_message = message.take_audioMessage();
            ChatMessageContent::Audio(FileInfo {
                url: audio_message.take_url(),
                mime: audio_message.take_mimetype(),
                sha256: audio_message.take_fileSha256(),
                enc_sha256: audio_message.take_fileEncSha256(),
                size: audio_message.get_fileLength() as usize,
                key: audio_message.take_mediaKey(),
            }, Duration::new(u64::from(audio_message.get_seconds()), 0))
        } else if message.has_documentMessage() {
            let mut document_message = message.take_documentMessage();
            ChatMessageContent::Document(FileInfo {
                url: document_message.take_url(),
                mime: document_message.take_mimetype(),
                sha256: document_message.take_fileSha256(),
                enc_sha256: document_message.take_fileEncSha256(),
                size: document_message.get_fileLength() as usize,
                key: document_message.take_mediaKey(),
            }, document_message.take_fileName())
        } else {
            ChatMessageContent::Text("TODO".to_string())
        })
    }

    pub fn into_proto(self) -> message_wire::Message {
        let mut message = message_wire::Message::new();
        match self {
            ChatMessageContent::Text(text) => message.set_conversation(text),
            ChatMessageContent::Image(info, size, thumbnail) => {
                let mut image_message = message_wire::ImageMessage::new();
                image_message.set_url(info.url);
                image_message.set_mimetype(info.mime);
                image_message.set_fileEncSha256(info.enc_sha256);
                image_message.set_fileSha256(info.sha256);
                image_message.set_fileLength(info.size as u64);
                image_message.set_mediaKey(info.key);
                image_message.set_height(size.0);
                image_message.set_width(size.1);
                image_message.set_jpegThumbnail(thumbnail);
                message.set_imageMessage(image_message);
            }
            ChatMessageContent::Document(info, filename) => {
                let mut document_message = message_wire::DocumentMessage::new();
                document_message.set_url(info.url);
                document_message.set_mimetype(info.mime);
                document_message.set_fileEncSha256(info.enc_sha256);
                document_message.set_fileSha256(info.sha256);
                document_message.set_fileLength(info.size as u64);
                document_message.set_mediaKey(info.key);
                document_message.set_fileName(filename);
                message.set_documentMessage(document_message);
            }
            _ => unimplemented!()
        }

        message
    }
}

#[derive(Debug)]
pub struct ChatMessage {
    pub direction: Direction,
    pub time: NaiveDateTime,
    pub id: MessageId,
    pub content: ChatMessageContent,
}

impl ChatMessage {
    pub fn from_proto_binary(content: &[u8]) -> Result<ChatMessage> {
        let webmessage = protobuf::parse_from_bytes::<message_wire::WebMessageInfo>(content).chain_err(|| "Invalid Protobuf chatmessage")?;
        ChatMessage::from_proto(webmessage)
    }


    pub fn from_proto(mut webmessage: message_wire::WebMessageInfo) -> Result<ChatMessage> {
        debug!("Processing WebMessageInfo: {:?}", &webmessage);
        let mut key = webmessage.take_key();

        Ok(ChatMessage {
            id: MessageId(key.take_id()),
            direction: Direction::parse(key)?,
            time: NaiveDateTime::from_timestamp(webmessage.get_messageTimestamp() as i64, 0),
            content: ChatMessageContent::from_proto(webmessage.take_message())?,
        })
    }

    pub fn into_proto_binary(self) -> Vec<u8> {
        let webmessage = self.into_proto();
        webmessage.write_to_bytes().unwrap()
    }

    pub fn into_proto(self) -> message_wire::WebMessageInfo {
        let mut webmessage = message_wire::WebMessageInfo::new();
        let mut key = message_wire::MessageKey::new();

        key.set_id(self.id.0);
        match self.direction {
            Direction::Sending(jid) => {
                key.set_remoteJid(jid.to_message_jid());
                key.set_fromMe(true);
            }
            Direction::Receiving(_) => unimplemented!()
        }
        webmessage.set_key(key);

        webmessage.set_messageTimestamp(self.time.timestamp() as u64);

        webmessage.set_message(self.content.into_proto());

        webmessage.set_status(message_wire::WebMessageInfo_STATUS::PENDING);
        debug!("Building WebMessageInfo: {:?}", &webmessage);

        webmessage
    }
}

impl Jid {
    pub fn to_message_jid(&self) -> String {
        self.id.to_string() + if self.is_group { "@g.us" } else { "@s.whatsapp.net" }
    }
}