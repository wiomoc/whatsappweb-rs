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
pub struct LocationMessage {
    // message fields
    pub degrees_latitude: f64,
    pub degrees_longitude: f64,
    pub name: String,
    pub address: String,
    pub url: String,
    pub jpeg_thumbnail: Vec<u8>,
}

#[derive(Debug)]
pub struct LiveLocationMessage {
    // message fields
    pub degrees_latitude: f64,
    pub degrees_longitude: f64,
    pub accuracy_in_meters: u32,
    pub speed_in_mps: f32,
    pub degrees_clockwise_from_magnetic_north: u32,
    pub caption: String,
    pub sequence_number: i64,
    pub jpeg_thumbnail: Vec<u8>,
}

#[derive(Debug)]
pub struct MessageKey {
    pub remote_jid: String,
    pub from_me: bool,
    pub id: String,
    pub participant: String,
}

#[derive(Debug, Serialize)]
pub struct ContactMessage {
    pub display_name: String,
    pub v_card: String,
}

#[derive(Debug, Serialize)]
pub struct ExtendedTextMessage {
    pub text: String,
    pub title: String,
    pub description: String,
    pub thumbnail: Vec<u8>,
}

#[derive(Debug)]
pub enum ChatMessageContent {
    Text(String),
    Image(FileInfo, (u32, u32), String, Vec<u8>),
    Audio(FileInfo, Duration),
    Document(FileInfo, String),
    Location(LocationMessage),
    LiveLocation(LiveLocationMessage),
    VideoMessage(FileInfo, (u32, u32), Duration, String, Vec<u8>),
    ProtocolMessage(MessageKey, String),
    ContactMessage(ContactMessage),
    ContactArrayMessage(String, Vec<ContactMessage>),
    CallMessage(Vec<u8>),
    ExtendedTextMessage(ExtendedTextMessage),
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
            }, (image_message.get_height(), image_message.get_width()), image_message.take_caption(), image_message.take_jpegThumbnail())
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
        } else if message.has_videoMessage() {
            let mut video_message = message.take_videoMessage();
            ChatMessageContent::VideoMessage(FileInfo {
                url: video_message.take_url(),
                mime: video_message.take_mimetype(),
                sha256: video_message.take_fileSha256(),
                enc_sha256: video_message.take_fileEncSha256(),
                size: video_message.get_fileLength() as usize,
                key: video_message.take_mediaKey(),
            },
                                             (video_message.get_height(), video_message.get_width()),
                                             Duration::new(u64::from(video_message.get_seconds()), 0),
                                             video_message.take_caption(),
                                             video_message.take_jpegThumbnail(),
            )
        } else if message.has_call() {
            let mut call_message = message.take_call();
            println!("{:?}", call_message);
            ChatMessageContent::CallMessage(
                call_message.take_callKey(),
            )
        } else if message.has_locationMessage() {
            let mut location_message = message.take_locationMessage();
            ChatMessageContent::Location(
                LocationMessage {
                    degrees_latitude: location_message.get_degreesLatitude(),
                    degrees_longitude: location_message.get_degreesLongitude(),
                    url: location_message.take_url(),
                    address: location_message.take_address(),
                    name: location_message.take_name(),
                    jpeg_thumbnail: location_message.take_jpegThumbnail(),
                }
            )
        } else if message.has_liveLocationMessage() {
            let mut live_location_message = message.take_liveLocationMessage();
            ChatMessageContent::LiveLocation(
                LiveLocationMessage {
                    degrees_latitude: live_location_message.get_degreesLatitude(),
                    degrees_longitude: live_location_message.get_degreesLongitude(),
                    accuracy_in_meters: live_location_message.get_accuracyInMeters(),
                    speed_in_mps: live_location_message.get_speedInMps(),
                    degrees_clockwise_from_magnetic_north: live_location_message.get_degreesClockwiseFromMagneticNorth(),
                    caption: live_location_message.take_caption(),
                    sequence_number: live_location_message.get_sequenceNumber(),
                    jpeg_thumbnail: live_location_message.take_jpegThumbnail(),
                }
            )
        } else if message.has_protocolMessage() {
            let mut protocol_message = message.take_protocolMessage();
            let mut key = protocol_message.take_key();

            ChatMessageContent::ProtocolMessage(
                MessageKey {
                    remote_jid: key.take_remoteJid(),
                    from_me: key.get_fromMe(),
                    id: key.take_id(),
                    participant: key.take_participant(),
                }, match protocol_message.get_field_type() {
                    message_wire::ProtocolMessage_TYPE::REVOKE => "REVOKE".to_string()
                },
            )
        } else if message.has_contactMessage() {
            let mut contact_message = message.take_contactMessage();
            ChatMessageContent::ContactMessage(ContactMessage {
                display_name: contact_message.take_displayName(),
                v_card: contact_message.take_vcard(),
            })
        } else if message.has_contactsArrayMessage() {
            let mut contact_array_message = message.take_contactsArrayMessage();
            let array_contacts = contact_array_message.take_contacts();

            let mut contacts = Vec::new();
            for contact in array_contacts.iter() {
                contacts.push(ContactMessage {
                    display_name: contact.get_displayName().to_string(),
                    v_card: contact.get_vcard().to_string(),
                });
            }

            ChatMessageContent::ContactArrayMessage(
                contact_array_message.take_displayName(),
                contacts,
            )
        } else if message.has_extendedTextMessage() {
            let mut extended_text_message = message.take_extendedTextMessage();
            ChatMessageContent::ExtendedTextMessage(ExtendedTextMessage {
                text: extended_text_message.take_text(),
                title: extended_text_message.take_title(),
                description: extended_text_message.take_description(),
                thumbnail: extended_text_message.take_jpegThumbnail(),
            })
        } else {
            warn!("Unknown message: {:?}", message);
            ChatMessageContent::Text("TODO".to_string())
        })
    }

    pub fn into_proto(self) -> message_wire::Message {
        let mut message = message_wire::Message::new();
        match self {
            ChatMessageContent::Text(text) => message.set_conversation(text),
            ChatMessageContent::Image(info, size, caption, thumbnail) => {
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
                image_message.set_caption(caption);
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