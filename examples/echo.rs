extern crate simple_logger;
#[macro_use]
extern crate log;
extern crate qrcode;
extern crate image;
extern crate bincode;
extern crate whatsappweb;
extern crate reqwest;
extern crate base64;

use std::fs::{File, OpenOptions, remove_file};
use std::io::{Read, Write, Cursor};
use std::sync::{RwLock, Arc};
use std::str::FromStr;


use image::Luma;

use whatsappweb::connection::*;
use whatsappweb::{Jid, Contact, PresenceStatus, GroupParticipantsChange, ChatAction, MediaType};
use whatsappweb::message::{MessageAck, MessageAckSide, MessageAckLevel, Direction, Peer, ChatMessageContent, ChatMessage};
use whatsappweb::crypto;
use whatsappweb::media;


const SESSION_FILENAME: &str = "session.bin";

struct Handler {}

impl WhatsappWebHandler for Handler {
    fn on_state_changed(&self, connection: &WhatsappWebConnection<Handler>, state: State) {
        info!("new state: {:?}", state);
    }

    fn on_persistent_session_data_changed(&self, persistent_session: PersistentSession) {
        bincode::serialize_into(OpenOptions::new().create(true).write(true).open(SESSION_FILENAME).unwrap(), &persistent_session).unwrap();
    }
    fn on_user_data_changed(&self, connection: &WhatsappWebConnection<Handler>, user_data: UserData) {
        info!("userdata changed: {:?}", user_data);
    }
    fn on_disconnect(&self, reason: whatsappweb::connection::DisconnectReason) {
        info!("disconnected");
        match reason {
            whatsappweb::connection::DisconnectReason::Removed => {
                remove_file(SESSION_FILENAME).unwrap();
            }
            _ => {}
        }
    }
    fn on_message(&self, connection: &WhatsappWebConnection<Handler>, message_new: bool, message: Box<ChatMessage>) {
        if !message_new {
            return;
        }

        let message = *message;

        let accepted_jid = Jid::from_str("491234567@c.us").unwrap();

        let peer = match message.direction {
            Direction::Receiving(peer) => peer,
            _ => return
        };

        match &peer {
            &Peer::Individual(ref jid) => if jid != &accepted_jid { return; }
            _ => return
        }

        connection.send_message_read(message.id.clone(), peer.clone());


        match message.content {
            ChatMessageContent::Text(text) => {
                connection.send_message(ChatMessageContent::Text(text), accepted_jid);
            }
            _ => {}
        }
    }
}

fn main() {
    let handler = Handler {};

    if let Ok(file) = File::open(SESSION_FILENAME) {
        let (_, join_handle) = whatsappweb::connection::with_persistent_session(bincode::deserialize_from(file).unwrap(), handler);
        join_handle.join().unwrap();
    } else {
        let (_, join_handle) = whatsappweb::connection::new(|qr| { qr.render::<Luma<u8>>().module_dimensions(10, 10).build().save("login_qr.png").unwrap(); }, handler);
        join_handle.join().unwrap();
    }
}
