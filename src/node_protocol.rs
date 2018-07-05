use std::collections::HashMap;

use Contact;
use Jid;
use Chat;
use ChatAction;
use PresenceStatus;
use GroupParticipantsChange;
use node_wire::{Node, NodeContent, IntoCow};
use message::{ChatMessage, MessageAck, MessageAckLevel, Peer, MessageId};
use errors::*;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MessageEventType {
    Relay,
    Last,
    Before,
    Set
}

#[derive(Debug)]
pub enum GroupCommand {
    Create(String),
    ParticipantsChange(Jid, GroupParticipantsChange),
    //TODO
    #[allow(dead_code)]
    Leave(Jid)
}

#[derive(Debug)]
pub enum AppEvent {
    Message(Box<ChatMessage>),

    MessageAck(MessageAck),
    // App only
    ContactDelete(Jid),
    //App only
    ContactAddChange(Contact),

    ChatAction(Jid, ChatAction),
    //App only
    Battery(u8),

    //Client only
    MessageRead { id: MessageId, peer: Peer },
    //Client only
    MessagePlayed { id: MessageId, peer: Peer },

    //Client only
    GroupCommand { inducer: Jid, id: String, participants: Vec<Jid>, command: GroupCommand },

    //Client only
    PresenceChange(PresenceStatus, Option<Jid>),
    //Client only
    StatusChange(String),
    //Client only
    NotifyChange(String),
    //Client only
    BlockProfile { unblock: bool, jid: Jid }
}

#[derive(Debug)]
pub enum Query {
    MessagesBefore { jid: Jid, id: String, count: u16 }
}

#[derive(Debug)]
pub enum AppMessage {
    MessagesEvents(Option<MessageEventType>, Vec<AppEvent>),
    //App only
    Contacts(Vec<Contact>),
    //App only
    Chats(Vec<Chat>),

    //Client only
    Query(Query)
}


impl AppMessage {
    pub fn deserialize(root_node: Node) -> Result<AppMessage> {
        let event_type = root_node.get_attribute("add").and_then(|add| MessageEventType::from_node(add.as_str())).ok();
        match root_node.desc() {
            "action" => {
                if let NodeContent::List(list) = root_node.content {
                    let mut app_events = Vec::with_capacity(list.len());
                    for mut node in list {
                        match node.desc() {
                            "message" => {
                                if let NodeContent::Binary(ref content) = node.content {
                                    app_events.push(AppEvent::Message(Box::new(ChatMessage::from_proto_binary(content)?)));
                                } else {
                                    bail!{ "invalid nodetype for chatmessage" }
                                }
                            }
                            "received" => {
                                app_events.push(AppEvent::MessageAck(
                                    MessageAck::from_app_message(MessageId(node.take_attribute("index")?.into_string()),
                                                                 MessageAckLevel::from_node(node.get_attribute("type")?.as_str())?,
                                                                 node.take_attribute("jid")?.into_jid()?,
                                                                 node.take_attribute("participant").and_then(|participant| participant.into_jid()).ok(),
                                                                 node.take_attribute("owner")?.as_str().parse().map_err(|_| "NAN")?)))
                            }
                            "read" => {
                                let jid = node.take_attribute("jid")?.into_jid()?;
                                app_events.push(AppEvent::ChatAction(jid, if node.take_attribute("type").ok().map_or(true, |typ| typ.as_str() != "false") {
                                    ChatAction::Read
                                } else {
                                    ChatAction::Unread
                                }));
                            }
                            "user" => {
                                let contact = Contact::parse_node(&mut node)?;
                                app_events.push(if contact.name.is_some() {
                                    AppEvent::ContactAddChange(contact)
                                } else {
                                    AppEvent::ContactDelete(contact.jid)
                                })
                            }
                            "chat" => {
                                let jid = node.take_attribute("jid")?.into_jid()?;
                                let action = ChatAction::from_node(&mut node)?;
                                app_events.push(AppEvent::ChatAction(jid, action));
                            }
                            "battery" => {
                                let level = node.take_attribute("value")?.as_str().parse().map_err(|_| "NAN")?;
                                app_events.push(AppEvent::Battery(level));
                            }
                            _ => {}
                        }
                    }

                    Ok(AppMessage::MessagesEvents(event_type, app_events))
                } else {
                    bail!{ "invalid or unsupported action type"}
                }
            }
            "response" => {
                match root_node.get_attribute("type")?.as_str() {
                    "contacts" => {
                        if let NodeContent::List(mut list) = root_node.content {
                            let mut contacts = Vec::with_capacity(list.len());
                            for mut node in list {
                                contacts.push(Contact::parse_node(&mut node)?);
                            }

                            Ok(AppMessage::Contacts(contacts))
                        } else {
                            bail!{ "Invalid nodetype for contacts"}
                        }
                    }
                    "chat" => {
                        if let NodeContent::List(mut list) = root_node.content {
                            let mut chats = Vec::with_capacity(list.len());
                            for mut node in list {
                                chats.push(Chat::parse_node(&mut node)?);
                            }

                            Ok(AppMessage::Chats(chats))
                        } else {
                            bail!{ "Invalid nodetype for chats"}
                        }
                    }
                    _ =>  bail!{ "invalid or unsupported 'response' type"}
                }
            }
            _ => bail!{ "invalid or unsupported app message type"}
        }
    }
    pub fn serialize(self, epoch: u32) -> Node {
        let mut attributes = HashMap::new();
        attributes.insert("epoch".cow(), NodeContent::String(epoch.to_string().cow()));

        match self {
            AppMessage::MessagesEvents(typ, events) => {
                attributes.insert("type".cow(), NodeContent::Token(typ.unwrap().into_node()));
                Node::new("action", attributes, NodeContent::List(
                    events.into_iter().map(|event| {
                        match event {
                            AppEvent::MessageRead { id, peer } => {
                                let mut attributes = HashMap::new();
                                attributes.insert("index".cow(), NodeContent::String(id.0.cow()));
                                match peer {
                                    Peer::Individual(jid) => {
                                        attributes.insert("jid".cow(), NodeContent::Jid(jid));
                                    }
                                    Peer::Group { group, participant } => {
                                        attributes.insert("jid".cow(), NodeContent::Jid(group));
                                        attributes.insert("participant".cow(), NodeContent::Jid(participant));
                                    }
                                }
                                attributes.insert("owner".cow(), NodeContent::Token("false"));
                                attributes.insert("count".cow(), NodeContent::String("1".cow()));
                                Node::new("read", attributes, NodeContent::None)
                            }
                            AppEvent::MessagePlayed { id, peer } => {
                                let mut attributes = HashMap::new();

                                attributes.insert("type".cow(), NodeContent::Token("played"));

                                attributes.insert("index".cow(), NodeContent::String(id.0.cow()));

                                match peer {
                                    Peer::Individual(jid) => { attributes.insert("from".cow(), NodeContent::Jid(jid)); }
                                    Peer::Group { group, participant } => {
                                        attributes.insert("from".cow(), NodeContent::Jid(group));
                                        attributes.insert("participant".cow(), NodeContent::Jid(participant));
                                    }
                                }
                                attributes.insert("owner".cow(), NodeContent::Token("false"));
                                attributes.insert("count".cow(), NodeContent::String("1".cow()));
                                Node::new("received", attributes, NodeContent::None)
                            }

                            AppEvent::Message(message) => {
                                Node::new("message", HashMap::new(), NodeContent::Binary(message.into_proto_binary()))
                            }
                            AppEvent::GroupCommand { inducer, id, participants, command } => {
                                let mut attributes = HashMap::new();
                                match command {
                                    GroupCommand::Create(subject) => {
                                        attributes.insert("subject".cow(), NodeContent::String(subject.cow()));
                                        attributes.insert("type".cow(), NodeContent::Token("create"));
                                    }
                                    GroupCommand::ParticipantsChange(jid, participants_change) => {
                                        attributes.insert("type".cow(), NodeContent::Token(participants_change.into_node()));
                                        attributes.insert("jid".cow(), NodeContent::Jid(jid));
                                    }
                                    GroupCommand::Leave(jid) => {
                                        attributes.insert("type".cow(), NodeContent::Token("leave"));
                                        attributes.insert("jid".cow(), NodeContent::Jid(jid));
                                    }
                                }
                                attributes.insert("author".cow(), NodeContent::Jid(inducer));
                                attributes.insert("id".cow(), NodeContent::String(id.cow()));
                                Node::new(
                                    "group",
                                    attributes,
                                    NodeContent::List(participants.into_iter().map(|jid| {
                                        let mut attributes = HashMap::new();
                                        attributes.insert("jid".cow(), NodeContent::Jid(jid));
                                        Node::new("participant", attributes, NodeContent::None)
                                    }).collect())
                                )
                            }
                            AppEvent::PresenceChange(status, jid) => {
                                let mut attributes = HashMap::new();
                                attributes.insert("type".cow(), NodeContent::Token(status.into_node()));
                                if let Some(jid) = jid {
                                    attributes.insert("to".cow(), NodeContent::Jid(jid));
                                }
                                Node::new("presence", attributes, NodeContent::None)
                            }
                            AppEvent::ChatAction(jid, action) => {
                                let mut attributes = HashMap::new();
                                attributes.insert("jid".cow(), NodeContent::Jid(jid));
                                match action {
                                    ChatAction::Pin(time) => {
                                        attributes.insert("type".cow(), NodeContent::String("pin".cow()));
                                        attributes.insert("pin".cow(), NodeContent::String(time.to_string().cow()));
                                    }
                                    //Fixme
                                    ChatAction::Unpin => {
                                        attributes.insert("type".cow(), NodeContent::String("pin".cow()));
                                        //attributes.insert("previous".to_string(), NodeContent::String(time.to_string()));
                                    }
                                    ChatAction::Mute(time) => {
                                        attributes.insert("type".cow(), NodeContent::Token("mute"));
                                        attributes.insert("mute".cow(), NodeContent::String(time.to_string().cow()));
                                    }
                                    //Fixme
                                    ChatAction::Unmute => {
                                        attributes.insert("type".cow(), NodeContent::Token("mute"));
                                    }
                                    ChatAction::Archive => {
                                        attributes.insert("type".cow(), NodeContent::Token("archive"));
                                    }
                                    ChatAction::Unarchive => {
                                        attributes.insert("type".cow(), NodeContent::Token("unarchive"));
                                    }

                                    _ => unimplemented!()
                                }

                                Node::new("chat", attributes, NodeContent::None)
                            }
                            AppEvent::StatusChange(status) => {
                                Node::new("status", HashMap::new(), NodeContent::String(status.cow()))
                            }
                            AppEvent::NotifyChange(name) => {
                                let mut node = Node::new_empty("profile");
                                node.set_attribute("name", NodeContent::String(name.cow()));
                                node
                            }
                            AppEvent::BlockProfile { unblock, jid } => {
                                let mut attributes = HashMap::new();
                                attributes.insert("jid".cow(), NodeContent::Jid(jid));

                                let user = Node::new("user", attributes, NodeContent::None);

                                let mut attributes = HashMap::new();
                                attributes.insert("type".cow(), NodeContent::Token(if unblock { "remove" } else { "add" }));
                                Node::new(
                                    "block",
                                    attributes,
                                    NodeContent::List(vec![user])
                                )
                            }
                            _ => unimplemented!()
                        }
                    }).collect())
                )
            }
            AppMessage::Query(query) => {
                match query {
                    Query::MessagesBefore { jid, id, count } => {
                        let mut node = Node::new_empty("query");
                        node.set_attribute("type", NodeContent::Token("message"));
                        node.set_attribute("kind", NodeContent::Token("before"));
                        node.set_attribute("jid", NodeContent::Jid(jid));
                        node.set_attribute("count", NodeContent::String(count.to_string().cow()));
                        node.set_attribute("index", NodeContent::String(id.cow()));
                        node.set_attribute("owner", NodeContent::Token("false"));
                        node
                    }
                }
            }
            _ => unreachable!()
        }
    }
}

pub fn parse_message_response(root_node: Node) -> Result<Vec<ChatMessage>> {
    if root_node.desc() == "response" && root_node.get_attribute("type").ok().map_or(false, |typ| typ.as_str() == "message") {
        if let NodeContent::List(nodes) = root_node.content {
            let mut messages = Vec::with_capacity(nodes.len());
            for node in nodes {
                if let NodeContent::Binary(ref content) = node.content {
                    messages.push(ChatMessage::from_proto_binary(content)?);
                } else {
                    bail!{ "invalid nodetype for chatmessage" }
                }
            }
            Ok(messages)
        } else {
            bail!{ "invalid nodetype for chatmessage" }
        }
    } else {
        bail!{ "invalid response" }
    }
}

impl Contact {
    fn parse_node(node: &mut Node) -> Result<Contact> {
        Ok(Contact {
            name: node.take_attribute("name").map(|name| name.into_string()).ok(),
            notify: node.take_attribute("notify").map(|notify| notify.into_string()).ok(),
            jid: node.take_attribute("jid")?.into_jid()?
        })
    }
}

impl Chat {
    fn parse_node(node: &mut Node) -> Result<Chat> {
        Ok(Chat {
            name: node.take_attribute("name").map(|name| name.into_string()).ok(),
            jid: node.take_attribute("jid")?.into_jid()?,
            last_activity: node.take_attribute("t")?.into_string().parse().map_err(|_| "NAN")?,
            spam: node.take_attribute("spam")?.into_string().parse().map_err(|_| "NAN")?,
            mute_until: node.take_attribute("mute").ok().and_then(|t| t.into_string().parse().ok()),
            pin_time: node.take_attribute("pin").ok().and_then(|t| t.into_string().parse().ok()),
            read_only: node.take_attribute("read_only").ok().and_then(|read_only| read_only.into_string().parse().ok()).unwrap_or(false),
        })
    }
}

impl MessageAckLevel {
    fn from_node(value: &str) -> Result<MessageAckLevel> {
        Ok(match value {
            "message" => MessageAckLevel::Received,
            "played" => MessageAckLevel::Played,
            "read" => MessageAckLevel::Read,
            _ => bail!{"invalid message ack level {}", value}
        })
    }
    #[allow(dead_code)]
    fn to_node(self) -> &'static str {
        match self {
            MessageAckLevel::Received => "message",
            MessageAckLevel::Played => "played",
            MessageAckLevel::Read => "read",
            _ => unimplemented!()
        }
    }
}

impl MessageEventType {
    fn from_node(value: &str) -> Result<MessageEventType> {
        Ok(match value {
            "last" => MessageEventType::Last,
            "before" => MessageEventType::Before,
            "relay" => MessageEventType::Relay,
            "set" => MessageEventType::Set,
            _ => bail!{"invalid message event type {}", value}
        })
    }
}

impl MessageEventType {
    fn into_node(self) -> &'static str {
        match self {
            MessageEventType::Last => "last",
            MessageEventType::Before => "before",
            MessageEventType::Relay => "relay",
            MessageEventType::Set => "set",
        }
    }
}

impl ChatAction {
    fn from_node(node: &mut Node) -> Result<ChatAction> {
        Ok(match node.take_attribute("type")?.as_str() {
            "spam" => ChatAction::Add,
            "delete" => ChatAction::Remove,
            "archive" => ChatAction::Archive,
            "unarchive" => ChatAction::Unarchive,
            "clear" => ChatAction::Clear,
            "pin" => {
                if let Ok(time) = node.take_attribute("pin") {
                    ChatAction::Pin(time.as_str().parse().map_err(|_| "NAN")?)
                } else {
                    ChatAction::Unpin
                }
            }
            "mute" => {
                if let Ok(time) = node.take_attribute("mute") {
                    ChatAction::Mute(time.as_str().parse().map_err(|_| "NAN")?)
                } else {
                    ChatAction::Unmute
                }
            }
            _ => bail!{ "invalid or unsupported chat action type"}
        })
    }
}


impl GroupParticipantsChange {
    fn into_node(self) -> &'static str {
        match self {
            GroupParticipantsChange::Add => "add",
            GroupParticipantsChange::Remove => "remote",
            GroupParticipantsChange::Promote => "promote",
            GroupParticipantsChange::Demote => "demote"
        }
    }
}

impl PresenceStatus {
    fn into_node(self) -> &'static str {
        match self {
            PresenceStatus::Unavailable => "unavailable",
            PresenceStatus::Available => "available",
            PresenceStatus::Typing => "composing",
            PresenceStatus::Recording => "recording",
        }
    }
}