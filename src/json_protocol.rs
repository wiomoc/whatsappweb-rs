use json::JsonValue;
use base64;
use super::{Jid, PresenceStatus, GroupMetadata, GroupParticipantsChange, MediaType};
use message::MessageAckLevel;

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
    pub fn deserialize(json: &'a JsonValue) -> Result<ServerMessage<'a>, ()> {
        let opcode = json[0].as_str().ok_or(())?;
        let payload = &json[1];

        Ok(match opcode {
            "Conn" => {
                ServerMessage::ConnectionAck {
                    user_jid: payload["wid"].as_str().and_then(|jid| Jid::from_str(jid).ok()).ok_or(())?,
                    server_token: payload["serverToken"].as_str().ok_or(())?,
                    client_token: payload["clientToken"].as_str().ok_or(())?,
                    secret: payload["secret"].as_str()
                }
            }
            "Cmd" => {
                let cmd_type = payload["type"].as_str().ok_or(())?;
                match cmd_type {
                    "challenge" => {
                        ServerMessage::ChallengeRequest(base64::decode(&payload["challenge"].as_str().ok_or(())?).map_err(|_| ())?)
                    }
                    "disconnect" => {
                        ServerMessage::Disconnect(payload["kind"].as_str())
                    }
                    "picture" => {
                        ServerMessage::PictureChange { jid: Jid::from_str(payload["jid"].as_str().ok_or(())?)?, removed: payload["tag"] == "removed" }
                    }
                    _ => return Err(())
                }
            }
            "Chat" => {
                let chat = Jid::from_str(payload["id"].as_str().ok_or(())?)?;
                let data = &payload["data"];
                let cmd_type = data[0].as_str().ok_or(())?;
                let inducer = data[1].as_str().and_then(|jid| Jid::from_str(jid).ok());
                match cmd_type {
                    typ @ "introduce" | typ @ "create" => {
                        let group_metadata_json = &data[2];
                        let admins_json = &group_metadata_json["admins"];
                        let regulars_json = &group_metadata_json["regulars"];

                        let mut participants = Vec::with_capacity(admins_json.len() + regulars_json.len());

                        for participant in admins_json.members() {
                            participants.push((Jid::from_str(participant.as_str().ok_or(())?)?, true));
                        }

                        for participant in regulars_json.members() {
                            participants.push((Jid::from_str(participant.as_str().ok_or(())?)?, false));
                        }

                        ServerMessage::GroupIntroduce {
                            inducer: inducer.ok_or(())?,
                            newly_created: typ == "create",
                            meta: GroupMetadata {
                                id: chat,
                                owner: None,
                                creation_time: group_metadata_json["creation"].as_i64().ok_or(())?,
                                subject: group_metadata_json["subject"].as_str().ok_or(())?.to_string(),
                                subject_owner: Jid::from_str(group_metadata_json["s_o"].as_str().ok_or(())?)?,
                                subject_time: group_metadata_json["s_t"].as_i64().ok_or(())?,
                                participants
                            }
                        }
                    }
                    "add" | "remove" | "promote" | "demote" => {
                        let participants_json = &data[2]["participants"];
                        let mut participants = Vec::with_capacity(participants_json.len());
                        for participant in participants_json.members() {
                            participants.push(Jid::from_str(participant.as_str().ok_or(())?)?)
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
                            subject_owner: inducer.ok_or(())?,
                            group: chat,
                            subject: subject_json["subject"].as_str().ok_or(())?.to_string(),
                            subject_time: subject_json["s_t"].as_i64().ok_or(())?
                        }
                    }
                    _ => return Err(())
                }
            }
            "Msg" | "MsgInfo" => {
                let cmd_type = payload["cmd"].as_str().ok_or(())?;
                match cmd_type {
                    "ack" => ServerMessage::MessageAck {
                        message_id: payload["id"].as_str().ok_or(())?,
                        sender: Jid::from_str(payload["from"].as_str().ok_or(())?)?,
                        receiver: Jid::from_str(payload["to"].as_str().ok_or(())?)?,
                        participant: payload["participant"].as_str().and_then(|jid| Jid::from_str(jid).ok()),
                        time: payload["t"].as_i64().ok_or(())?,
                        level: MessageAckLevel::from_json(payload["ack"].as_u8().ok_or(())?)?
                    },
                    "acks" => ServerMessage::MessageAcks {
                        message_ids: payload["id"].members().map(|id| id.as_str().unwrap()).collect(),
                        sender: Jid::from_str(payload["from"].as_str().ok_or(())?)?,
                        receiver: Jid::from_str(payload["to"].as_str().ok_or(())?)?,
                        participant: payload["participant"].as_str().and_then(|jid| Jid::from_str(jid).ok()),
                        time: payload["t"].as_i64().ok_or(())?,
                        level: MessageAckLevel::from_json(payload["ack"].as_u8().ok_or(())?)?
                    },
                    _ => return Err(())
                }
            }
            "Presence" => {
                ServerMessage::PresenceChange {
                    jid: Jid::from_str(payload["id"].as_str().ok_or(())?)?,
                    status: PresenceStatus::from_json(payload["type"].as_str().ok_or(())?)?,
                    time: payload["t"].as_i64()
                }
            }
            "Status" => {
                ServerMessage::StatusChange(Jid::from_str(payload["id"].as_str().ok_or(())?)?, payload["status"].as_str().ok_or(())?.to_string())
            }
            _ => return Err(())
        })
    }
}

impl MessageAckLevel {
    fn from_json(value: u8) -> Result<MessageAckLevel, ()> {
        Ok(match value {
            0 => MessageAckLevel::PendingSend,
            1 => MessageAckLevel::Send,
            2 => MessageAckLevel::Received,
            3 => MessageAckLevel::Read,
            4 => MessageAckLevel::Played,
            _ => return Err(())
        })
    }
}

impl PresenceStatus {
    fn from_json(value: &str) -> Result<PresenceStatus, ()> {
        Ok(match value {
            "unavailable" => PresenceStatus::Unavailable,
            "available" => PresenceStatus::Available,
            "composing" => PresenceStatus::Typing,
            "recording" => PresenceStatus::Recording,
            _ => return Err(())
        })
    }
}

impl GroupMetadata {
    fn from_json(value: &JsonValue) -> Result<GroupMetadata, ()> {
        let participants_json = &value["participants"];
        let mut participants = Vec::with_capacity(participants_json.len());
        for participant in participants_json.members() {
            participants.push((Jid::from_str(participant["id"].as_str().ok_or(())?)?, participant["isAdmin"].as_bool().ok_or(())?));
        }

        Ok(GroupMetadata {
            id: Jid::from_str(value["id"].as_str().ok_or(())?)?,
            creation_time: value["creation"].as_i64().ok_or(())?,
            owner: Some(Jid::from_str(value["owner"].as_str().ok_or(())?)?),
            participants,
            subject: value["subject"].as_str().ok_or(())?.to_string(),
            subject_time: value["subjectTime"].as_i64().ok_or(())?,
            subject_owner: Jid::from_str(value["subjectOwner"].as_str().ok_or(())?)?
        })
    }
}

impl GroupParticipantsChange {
    fn from_json(value: &str) -> Result<GroupParticipantsChange, ()> {
        Ok(match value {
            "add" => GroupParticipantsChange::Add,
            "remove" => GroupParticipantsChange::Remove,
            "promote" => GroupParticipantsChange::Promote,
            "demote" => GroupParticipantsChange::Demote,
            _ => return Err(())
        })
    }
}

pub fn parse_response_status(response: &JsonValue) -> Result<(), u16> {
    response["status"].as_u16().map_or(Ok(()), |status_code| if status_code == 200 {
        Ok(())
    } else {
        Err(status_code)
    })
}

pub fn build_init_request(client_id: &str) -> JsonValue {
    array!["admin", "init", array![0, 2, 9457], array!["ww-rs", "ww-rs"], client_id, true]
}

pub fn parse_init_response<'a>(response: &'a JsonValue) -> Result<&'a str, ()> {
    parse_response_status(response).map_err(|_| ())?;
    response["ref"].as_str().ok_or(())
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

pub fn parse_file_upload_response<'a>(response: &'a JsonValue) -> Result<&'a str, ()> {
    parse_response_status(response).map_err(|_| ())?;
    response["url"].as_str().ok_or(())
}

pub fn build_profile_picture_request(jid: &Jid) -> JsonValue {
    array!["query", "ProfilePicThumb", jid.to_string()]
}

pub fn parse_profile_picture_response<'a>(response: &'a JsonValue) -> Option<&'a str> {
    response["eurl"].as_str()
}

pub fn build_profile_status_request(jid: &Jid) -> JsonValue {
    array!["query", "Status", jid.to_string()]
}

pub fn parse_profile_status_response<'a>(response: &'a JsonValue) -> Option<&'a str> {
    response["status"].as_str()
}

pub fn build_group_metadata_request(jid: &Jid) -> JsonValue {
    array!["query", "GroupMetadata", jid.to_string()]
}

pub fn parse_group_metadata_response<'a>(response: &'a JsonValue) -> Result<GroupMetadata, ()> {
    parse_response_status(response).map_err(|_| ())?;
    GroupMetadata::from_json(response)
}
