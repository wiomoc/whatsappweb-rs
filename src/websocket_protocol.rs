use std::str;
use std::borrow::Cow;
use std::borrow::Borrow;
use std::ops::Deref;

use ws::Message;
use json;
use json::JsonValue;

#[derive(Copy, Clone, PartialEq)]
#[allow(dead_code)]
pub enum WebsocketMessageMetric {
    None = 0,
    DebugLog = 1,
    QueryResume = 2,
    QueryReceipt = 3,
    QueryMedia = 4,
    QueryChat = 5,
    QueryContacts = 6,
    QueryMessages = 7,
    Presence = 8,
    PresenceSubscribe = 9,
    Group = 10,
    Read = 11,
    Chat = 12,
    Received = 13,
    Pic = 14,
    Status = 15,
    Message = 16,
    QueryActions = 17,
    Block = 18,
    QueryGroup = 19,
    QueryPreview = 20,
    QueryEmoji = 21,
    QueryMessageInfo = 22,
    Spam = 23,
    QuerySearch = 24,
    QueryIdentity = 25,
    QueryUrl = 26,
    Profile = 27,
    Contact = 28,
    QueryVcard = 29,
    QueryStatus = 30,
    QueryStatusUpdate = 31,
    PrivacyStatus = 32,
    QueryLiveLocations = 33,
    LiveLocation = 34,
    QueryVname = 35,
    QueryLabels = 36,
    Call = 37,
    QueryCall = 38,
    QueryQuickReplies = 39
}

pub struct WebsocketMessage<'a> {
    pub tag: Cow<'a, str>,
    pub payload: WebsocketMessagePayload<'a>
}

pub enum WebsocketMessagePayload<'a> {
    Json(JsonValue),
    BinarySimple(&'a [u8]),
    BinaryEphemeral(WebsocketMessageMetric, &'a [u8]),
    Empty,
    Pong
}


impl<'a> WebsocketMessage<'a> {
    #[inline]
    pub fn serialize(&self) -> Message {
        match self.payload {
            WebsocketMessagePayload::Json(ref json) => {
                Message::Text([self.tag.deref(), ",", json.to_string().as_str()].concat())
            }
            WebsocketMessagePayload::BinarySimple(ref binary) => {
                Message::Binary([self.tag.deref().as_bytes(), b",", binary].concat())
            }
            WebsocketMessagePayload::BinaryEphemeral(metric, ref binary) => {
                if metric != WebsocketMessageMetric::None {
                    Message::Binary([self.tag.deref().as_bytes(), b",", &[metric as u8], b"\x80", binary].concat())
                } else {
                    Message::Binary([self.tag.deref().as_bytes(), b",,", binary].concat())
                }
            }
            WebsocketMessagePayload::Empty => {
                Message::Text([self.tag.borrow(), ","].concat())
            }
            WebsocketMessagePayload::Pong => unimplemented!()
        }
    }

    #[inline]
    pub fn deserialize(message: &'a Message) -> Result<WebsocketMessage<'a>, ()> {
        match *message {
            Message::Text(ref message) => {
                if let Some(sep) = message.find(',') {
                    let (tag_str, payload) = message.split_at(sep + 1);
                    let tag = Cow::Borrowed(tag_str.split_at(sep).0);

                    Ok(if payload.is_empty() {
                        WebsocketMessage { tag, payload: WebsocketMessagePayload::Empty }
                    } else {
                        WebsocketMessage { tag, payload: WebsocketMessagePayload::Json(json::parse(payload).map_err(|_| ())?) }
                    })
                } else if message.get(0..1).map_or(false, |first| first == "!") {
                    Ok(WebsocketMessage { tag: Cow::Borrowed(""), payload: WebsocketMessagePayload::Pong })
                } else {
                    Err(())
                }
            }
            Message::Binary(ref message) => {
                if let Some(sep) = message.iter().position(|x| x == &b',') {
                    Ok(WebsocketMessage {
                        tag: Cow::Borrowed(str::from_utf8(&message[..sep]).map_err(|_| ())?),
                        payload: WebsocketMessagePayload::BinarySimple(&message[(sep + 1)..])
                    })
                } else {
                    Err(())
                }
            }
        }
    }
}