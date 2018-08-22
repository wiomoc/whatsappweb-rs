extern crate ws;
extern crate simple_logger;
#[macro_use]
extern crate log;
extern crate url;
#[macro_use]
extern crate json;
extern crate ring;
extern crate base64;
extern crate qrcode;
extern crate image;
extern crate untrusted;
#[macro_use]
extern crate serde_derive;
extern crate bincode;
extern crate protobuf;
extern crate byteorder;
extern crate chrono;
#[macro_use]
extern crate error_chain;
#[cfg(feature = "media")]
extern crate reqwest;

pub mod connection;
pub mod message;
#[cfg(feature = "media")]
pub mod media;
mod message_wire;
mod node_protocol;
mod node_wire;
mod json_protocol;
mod websocket_protocol;
pub mod crypto;
mod timeout;
pub mod errors;

use std::str::FromStr;

use errors::*;


#[derive(Debug, Clone, PartialOrd, PartialEq)]
pub struct Jid {
    pub id: String,
    pub is_group: bool,
}

/// Jid used to identify either a group or an individual
impl Jid {
    pub fn to_string(&self) -> String {
        self.id.to_string() + if self.is_group { "@g.us" } else { "@c.us" }
    }

    /// If the Jid is from an individual return the international phonenumber, else None
    pub fn phonenumber(&self) -> Option<String> {
        if !self.is_group {
            Some("+".to_string() + &self.id)
        } else {
            None
        }
    }

    pub fn from_phonenumber(mut phonenumber: String) -> Result<Jid> {
        if phonenumber.starts_with('+') {
            phonenumber.remove(0);
        }

        if phonenumber.chars().any(|c| !c.is_digit(10)) {
            return Err("not a valid phonenumber".into());
        }

        Ok(Jid { id: phonenumber, is_group: false })
    }
}

impl FromStr for Jid {
    type Err = Error;

    fn from_str(jid: &str) -> Result<Jid> {
        let at = jid.find('@').ok_or("jid missing @")?;

        let (id, surfix) = jid.split_at(at);
        Ok(Jid {
            id: id.to_string(),
            is_group: match surfix {
                "@c.us" => false,
                "@g.us" => true,
                "@s.whatsapp.net" => false,
                "@broadcast" => false, //TODO
                _ => return Err("invalid surfix".into())
            },
        })
    }
}

#[derive(Debug)]
pub struct Contact {
    ///name used in phonebook, set by user
    pub name: Option<String>,
    ///name used in pushnotification, set by opposite peer
    pub notify: Option<String>,
    pub jid: Jid,
}

#[derive(Debug)]
pub struct Chat {
    pub name: Option<String>,
    pub jid: Jid,
    pub last_activity: i64,
    pub pin_time: Option<i64>,
    pub mute_until: Option<i64>,
    pub spam: bool,
    pub read_only: bool,
}


#[derive(Debug, Copy, Clone)]
pub enum PresenceStatus {
    Unavailable,
    Available,
    Typing,
    Recording,
}

#[derive(Debug)]
pub struct GroupMetadata {
    pub creation_time: i64,
    pub id: Jid,
    pub owner: Option<Jid>,
    pub participants: Vec<(Jid, bool)>,
    pub subject: String,
    pub subject_owner: Jid,
    pub subject_time: i64,
}

#[derive(Debug, Copy, Clone)]
pub enum GroupParticipantsChange {
    Add,
    Remove,
    Promote,
    Demote,
}

#[derive(Debug, Copy, Clone)]
pub enum ChatAction {
    Add,
    Remove,
    Archive,
    Unarchive,
    Clear,
    Pin(i64),
    Unpin,
    Mute(i64),
    Unmute,
    Read,
    Unread,
}

#[derive(Copy, Clone)]
pub enum MediaType {
    Image,
    Video,
    Audio,
    Document,
}