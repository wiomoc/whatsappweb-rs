use std::str::FromStr;

use json::JsonValue;
use base64;

use super::{Jid, PresenceStatus, GroupMetadata, GroupParticipantsChange, MediaType};
use message::MessageAckLevel;
use errors::*;


#[derive(Debug)]
pub enum ServerMessage<'a> {
    ConnectionAck { user_jid: Jid, client_token: &'a str, server_token: &'a str, secret: Option<&'a str> },
    ChallengeRequest(Vec<u8>),
    Disconnect(Option<&'a str>),
    PresenceChange { jid: Jid, status: PresenceStatus, time: Option<i64> },
    MessageAck { message_id: &'a str, level: MessageAckLevel, sender: Jid, receiver: Jid, participant: Option<Jid>, time: i64 },
    MessageAcks { message_ids: Vec<&'a str>, level: MessageAckLevel, sender: Jid, receiver: Jid, participant: Option<Jid>, time: i64 },
    GroupIntroduce { newly_created: bool, inducer: Jid, meta: GroupMetadata },
    GroupParticipantsChange { group: Jid, change: GroupParticipantsChange, inducer: Option<Jid>, participants: Vec<Jid> },
    GroupSubjectChange { group: Jid, subject: String, subject_time: i64, subject_owner: Jid },
    PictureChange { jid: Jid, removed: bool },
    StatusChange(Jid, String)
}


impl<'a> ServerMessage<'a> {
    #[inline]
    pub fn deserialize(json: &'a JsonValue) -> Result<ServerMessage<'a>> {
        let opcode = json[0].as_str().ok_or("server message without opcode")?;
        let payload = &json[1];

        Ok(match opcode {
            "Conn" => {
                ServerMessage::ConnectionAck {
                    user_jid: payload.get_str("wid").and_then(|jid| Jid::from_str(jid))?,
                    server_token: payload.get_str("serverToken")?,
                    client_token: payload.get_str("clientToken")?,
                    secret: payload["secret"].as_str()
                }
            }
            "Cmd" => {
                let cmd_type = payload.get_str("type")?;
                match cmd_type {
                    "challenge" => {
                        ServerMessage::ChallengeRequest(base64::decode(&payload.get_str("challenge")?)?)
                    }
                    "disconnect" => {
                        ServerMessage::Disconnect(payload["kind"].as_str())
                    }
                    "picture" => {
                        ServerMessage::PictureChange { jid: Jid::from_str(payload.get_str("jid")?)?, removed: payload["tag"] == "removed" }
                    }
                    _ => bail! { "invalid or unsupported 'Cmd' subcommand type {}", cmd_type}
                }
            }
            "Chat" => {
                let chat = Jid::from_str(payload.get_str("id")?)?;
                let data = &payload["data"];
                let cmd_type = data[0].as_str().ok_or("chat command without subcommand")?;
                let inducer = data[1].as_str().and_then(|jid| Jid::from_str(jid).ok());
                match cmd_type {
                    typ @ "introduce" | typ @ "create" => {
                        let group_metadata_json = &data[2];
                        let admins_json = &group_metadata_json["admins"];
                        let regulars_json = &group_metadata_json["regulars"];

                        let mut participants = Vec::with_capacity(admins_json.len() + regulars_json.len());

                        for participant in admins_json.members() {
                            participants.push((Jid::from_str(participant.as_str().ok_or("not a string")?)?, true));
                        }

                        for participant in regulars_json.members() {
                            participants.push((Jid::from_str(participant.as_str().ok_or("not a string")?)?, false));
                        }

                        ServerMessage::GroupIntroduce {
                            inducer: inducer.ok_or("missing inducer")?,
                            newly_created: typ == "create",
                            meta: GroupMetadata {
                                id: chat,
                                owner: None,
                                creation_time: group_metadata_json.get_i64("creation")?,
                                subject: group_metadata_json.get_str("subject")?.to_string(),
                                subject_owner: Jid::from_str(group_metadata_json.get_str("s_o")?)?,
                                subject_time: group_metadata_json.get_i64("s_t")?,
                                participants
                            }
                        }
                    }
                    "add" | "remove" | "promote" | "demote" => {
                        let participants_json = &data[2]["participants"];
                        let mut participants = Vec::with_capacity(participants_json.len());
                        for participant in participants_json.members() {
                            participants.push(Jid::from_str(participant.as_str().ok_or("not a string")?)?)
                        }
                        ServerMessage::GroupParticipantsChange {
                            inducer,
                            group: chat,
                            participants,
                            change: GroupParticipantsChange::from_json(cmd_type).unwrap()
                        }
                    }
                    "subject" => {
                        let subject_json = &data[2];
                        ServerMessage::GroupSubjectChange {
                            subject_owner: inducer.ok_or("missing inducer")?,
                            group: chat,
                            subject: subject_json.get_str("subject")?.to_string(),
                            subject_time: subject_json.get_i64("s_t")?
                        }
                    }
                    _ => bail! { "invalid or unsupported 'Chat' subcommand type {}", cmd_type}
                }
            }
            "Msg" | "MsgInfo" => {
                let cmd_type = payload.get_str("cmd")?;
                match cmd_type {
                    "ack" => ServerMessage::MessageAck {
                        message_id: payload.get_str("id")?,
                        sender: Jid::from_str(payload.get_str("from")?)?,
                        receiver: Jid::from_str(payload.get_str("to")?)?,
                        participant: payload["participant"].as_str().and_then(|jid| Jid::from_str(jid).ok()),
                        time: payload.get_i64("t")?,
                        level: MessageAckLevel::from_json(payload.get_u8("ack")?)?
                    },
                    "acks" => ServerMessage::MessageAcks {
                        message_ids: payload["id"].members().map(|id| id.as_str().unwrap()).collect(),
                        sender: Jid::from_str(payload.get_str("from")?)?,
                        receiver: Jid::from_str(payload.get_str("to")?)?,
                        participant: payload["participant"].as_str().and_then(|jid| Jid::from_str(jid).ok()),
                        time: payload.get_i64("t")?,
                        level: MessageAckLevel::from_json(payload.get_u8("ack")?)?
                    },
                    _ => bail! { "invalid or unsupported 'Msg' or 'MsgInfo' subcommand type {}", cmd_type}
                }
            }
            "Presence" => {
                ServerMessage::PresenceChange {
                    jid: Jid::from_str(payload.get_str("id")?)?,
                    status: PresenceStatus::from_json(payload.get_str("type")?)?,
                    time: payload["t"].as_i64()
                }
            }
            "Status" => {
                ServerMessage::StatusChange(Jid::from_str(payload.get_str("id")?)?, payload.get_str("status")?.to_string())
            }
            _ => bail! { "invalid or unsupported opcode {}", opcode}
        })
    }
}

impl MessageAckLevel {
    fn from_json(value: u8) -> Result<MessageAckLevel> {
        Ok(match value {
            0 => MessageAckLevel::PendingSend,
            1 => MessageAckLevel::Send,
            2 => MessageAckLevel::Received,
            3 => MessageAckLevel::Read,
            4 => MessageAckLevel::Played,
            _ => bail! {"Invalid message ack level {}", value}
        })
    }
}

impl PresenceStatus {
    fn from_json(value: &str) -> Result<PresenceStatus> {
        Ok(match value {
            "unavailable" => PresenceStatus::Unavailable,
            "available" => PresenceStatus::Available,
            "composing" => PresenceStatus::Typing,
            "recording" => PresenceStatus::Recording,
            _ => bail! {"Invalid presence status {}", value}
        })
    }
}

impl GroupMetadata {
    fn from_json(value: &JsonValue) -> Result<GroupMetadata> {
        let participants_json = &value["participants"];
        let mut participants = Vec::with_capacity(participants_json.len());
        for participant in participants_json.members() {
            participants.push((Jid::from_str(participant.get_str("id")?)?, participant.get_bool("isAdmin")?));
        }

        Ok(GroupMetadata {
            id: Jid::from_str(value.get_str("id")?)?,
            creation_time: value.get_i64("creation")?,
            owner: Some(Jid::from_str(value.get_str("owner")?)?),
            participants,
            subject: value.get_str("subject")?.to_string(),
            subject_time: value.get_i64("subjectTime")?,
            subject_owner: Jid::from_str(value.get_str("subjectOwner")?)?
        })
    }
}

impl GroupParticipantsChange {
    fn from_json(value: &str) -> Result<GroupParticipantsChange> {
        Ok(match value {
            "add" => GroupParticipantsChange::Add,
            "remove" => GroupParticipantsChange::Remove,
            "promote" => GroupParticipantsChange::Promote,
            "demote" => GroupParticipantsChange::Demote,
            _ => bail! {"invalid group command {}", value}
        })
    }
}

pub fn parse_response_status(response: &JsonValue) -> Result<()> {
    response["status"].as_u16().map_or(Ok(()), |status_code| if status_code == 200 {
        Ok(())
    } else {
        bail! {"received status code {}", status_code}
    })
}

pub fn build_init_request(client_id: &str) -> JsonValue {
    array!["admin", "init", array![0, 3, 416], array!["ww-rs", "ww-rs"], client_id, true]
}

pub fn parse_init_response<'a>(response: &'a JsonValue) -> Result<&'a str> {
    parse_response_status(response)?;
    response.get_str("ref")
}

pub fn build_takeover_request(client_token: &str, server_token: &str, client_id: &str) -> JsonValue {
    array!["admin", "login", client_token, server_token, client_id, "takeover"]
}

pub fn build_challenge_response(server_token: &str, client_id: &str, signature: &[u8]) -> JsonValue {
    array!["admin","challenge", base64::encode(&signature), server_token, client_id]
}

pub fn build_presence_subscribe(jid: &Jid) -> JsonValue {
    array!["action", "presence", "subscribe", jid.to_string()]
}

pub fn build_file_upload_request(hash: &[u8], media_type: MediaType) -> JsonValue {
    array!["action", "encr_upload", match media_type {
        MediaType::Image => "image",
        MediaType::Video => "video",
        MediaType::Audio => "audio",
        MediaType::Document => "document",
    }, base64::encode(hash)]
}

pub fn parse_file_upload_response<'a>(response: &'a JsonValue) -> Result<&'a str> {
    parse_response_status(response)?;
    response.get_str("url")
}

pub fn build_profile_picture_request(jid: &Jid) -> JsonValue {
    array!["query", "ProfilePicThumb", jid.to_string()]
}

pub fn parse_profile_picture_response(response: &JsonValue) -> Option<&str> {
    response["eurl"].as_str()
}

pub fn build_profile_status_request(jid: &Jid) -> JsonValue {
    array!["query", "Status", jid.to_string()]
}

pub fn parse_profile_status_response(response: &JsonValue) -> Option<&str> {
    response["status"].as_str()
}

pub fn build_group_metadata_request(jid: &Jid) -> JsonValue {
    array!["query", "GroupMetadata", jid.to_string()]
}

pub fn parse_group_metadata_response(response: &JsonValue) -> Result<GroupMetadata> {
    parse_response_status(response)?;
    GroupMetadata::from_json(response)
}

pub trait JsonNonNull {
    fn get_str(&self, field: &'static str) -> Result<&str>;
    fn get_i64<'a>(&'a self, field: &'static str) -> Result<i64>;
    fn get_u8<'a>(&'a self, field: &'static str) -> Result<u8>;
    fn get_bool<'a>(&'a self, field: &'static str) -> Result<bool>;
}

impl JsonNonNull for JsonValue {
    fn get_str<'a>(&'a self, field: &'static str) -> Result<&'a str> {
        self[field].as_str().ok_or_else(|| ErrorKind::JsonFieldMissing(field).into())
    }

    fn get_i64<'a>(&'a self, field: &'static str) -> Result<i64> {
        self[field].as_i64().ok_or_else(|| ErrorKind::JsonFieldMissing(field).into())
    }

    fn get_u8<'a>(&'a self, field: &'static str) -> Result<u8> {
        self[field].as_u8().ok_or_else(|| ErrorKind::JsonFieldMissing(field).into())
    }

    fn get_bool<'a>(&'a self, field: &'static str) -> Result<bool> {
        self[field].as_bool().ok_or_else(|| ErrorKind::JsonFieldMissing(field).into())
    }
}
