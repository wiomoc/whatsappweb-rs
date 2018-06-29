use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::collections::HashMap;
use std::io::{Read, Write, Result, Cursor};
use std::char;
use Jid;
use std::borrow::Cow;
use std::ops::Deref;


const LIST_EMPTY: u8 = 0;
const STREAM_END: u8 = 2;
const DICTIONARY_0: u8 = 236;
const DICTIONARY_1: u8 = 237;
const DICTIONARY_2: u8 = 238;
const DICTIONARY_3: u8 = 239;
const LIST_8: u8 = 248;
const LIST_16: u8 = 249;
const JID_PAIR: u8 = 250;
const HEX_8: u8 = 251;
const BINARY_8: u8 = 252;
const BINARY_20: u8 = 253;
const BINARY_32: u8 = 254;
const NIBBLE_8: u8 = 255;
const PACKED_MAX: u8 = 254;

const TOKENS: [&str; 148] = ["200", "400", "404", "500", "501", "502", "action", "add",
    "after", "archive", "author", "available", "battery", "before", "body",
    "broadcast", "chat", "clear", "code", "composing", "contacts", "count",
    "create", "debug", "delete", "demote", "duplicate", "encoding", "error",
    "false", "filehash", "from", "g.us", "group", "groups_v2", "height", "id",
    "image", "in", "index", "invis", "item", "jid", "kind", "last", "leave",
    "live", "log", "media", "message", "mimetype", "missing", "modify", "name",
    "notification", "notify", "out", "owner", "participant", "paused",
    "picture", "played", "presence", "preview", "promote", "query", "raw",
    "read", "receipt", "received", "recipient", "recording", "relay",
    "remove", "response", "resume", "retry", "c.us", "seconds",
    "set", "size", "status", "subject", "subscribe", "t", "text", "to", "true",
    "type", "unarchive", "unavailable", "url", "user", "value", "web", "width",
    "mute", "read_only", "admin", "creator", "short", "update", "powersave",
    "checksum", "epoch", "block", "previous", "409", "replaced", "reason",
    "spam", "modify_tag", "message_info", "delivery", "emoji", "title",
    "description", "canonical-url", "matched-text", "star", "unstar",
    "media_key", "filename", "identity", "unread", "page", "page_count",
    "search", "media_message", "security", "call_log", "profile", "ciphertext",
    "invite", "gif", "vcard", "frequent", "privacy", "blacklist", "whitelist",
    "verify", "location", "document", "elapsed", "revoke_invite", "expiration",
    "unsubscribe", "disable"
];

#[derive(Debug, PartialEq, Clone)]
pub enum NodeContent {
    None,
    List(Vec<Node>),
    String(Cow<'static, str>),
    Binary(Vec<u8>),
    Jid(Jid),
    Token(&'static str),
    Nibble(Cow<'static, str>)
}

impl NodeContent {
    pub fn into_cow(self) -> Cow<'static, str> {
        match self {
            NodeContent::None => "".cow(),
            NodeContent::List(_) => unimplemented!(),
            NodeContent::String(string) => string,
            NodeContent::Nibble(string) => string,
            NodeContent::Binary(_) => unimplemented!(),
            NodeContent::Jid(jid) => Cow::Owned(jid.to_string()),
            NodeContent::Token(ref token) => Cow::Borrowed(token)
        }
    }

    pub fn into_string(self) -> String {
        match self {
            NodeContent::None => "".to_string(),
            NodeContent::List(_) => unimplemented!(),
            NodeContent::String(string) => string.into(),
            NodeContent::Nibble(string) => string.into(),
            NodeContent::Binary(_) => unimplemented!(),
            NodeContent::Jid(jid) => jid.to_string(),
            NodeContent::Token(ref token) => token.to_string()
        }
    }

    pub fn into_jid(self) -> Option<Jid> {
        match self {
            NodeContent::Jid(jid) => Some(jid),
            _ => None
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            &NodeContent::None => "",
            &NodeContent::List(_) => unimplemented!(),
            &NodeContent::String(ref string) => string.deref(),
            &NodeContent::Nibble(ref string) => string.deref(),
            &NodeContent::Binary(_) => unimplemented!(),
            &NodeContent::Jid(_) => unimplemented!(),//jid.to_string().as_str()
            &NodeContent::Token(ref token) => token
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Node {
    pub desc: Cow<'static, str>,
    pub attributes: HashMap<Cow<'static, str>, NodeContent>,
    pub content: NodeContent
}

fn read_list_size(tag: u8, stream: &mut Read) -> Result<u16> {
    match tag {
        LIST_EMPTY => Ok(0),
        LIST_8 => Ok(stream.read_u8()? as u16),
        LIST_16 => stream.read_u16::<LittleEndian>(),
        _ => Ok(0)
    }
}

fn write_list_size(size: u16, stream: &mut Write) -> Result<()> {
    match size {
        0 => { stream.write_u8(LIST_EMPTY)?; }
        1 ... 256 => {
            stream.write_u8(LIST_8)?;
            stream.write_u8(size as u8)?;
        }
        _ => {
            stream.write_u8(LIST_16)?;
            stream.write_u16::<LittleEndian>(size)?;
        }
    }
    Ok(())
}

fn read_list(tag: u8, stream: &mut Read) -> Result<Vec<Node>> {
    let size = read_list_size(tag, stream)?;
    let mut list = Vec::<Node>::with_capacity(size as usize);

    for _ in 0..size {
        list.push(Node::deserialize_stream(stream)?);
    }

    Ok(list)
}

fn write_list(list: Vec<Node>, stream: &mut Write) -> Result<()> {
    write_list_size(list.len() as u16, stream)?;

    for node in list {
        node.serialize_stream(stream)?
    }

    Ok(())
}

fn nibble_to_char(nibble: u8) -> char {
    match nibble {
        0 ... 9 =>
            char::from_digit(nibble as u32, 10).unwrap().to_ascii_uppercase(),
        10 => '-',
        11 => '.',
        15 => '\0',
        _ => {
            unimplemented!()
        }
    }
}

fn char_to_nibble(nibble: char) -> u8 {
    match nibble {
        '0' => 0,
        '1' => 1,
        '2' => 2,
        '3' => 3,
        '4' => 4,
        '5' => 5,
        '6' => 6,
        '7' => 7,
        '8' => 8,
        '9' => 9,
        '-' => 10,
        '.' => 11,
        '\0' => 15,
        _ => {
            unimplemented!()
        }
    }
}

fn read_node_content(tag: u8, stream: &mut Read) -> Result<NodeContent> {
    Ok(match tag {
        3 ... 151 => NodeContent::Token(TOKENS[(tag - 3) as usize]),
        DICTIONARY_0 | DICTIONARY_1 | DICTIONARY_2 | DICTIONARY_3 => {
            stream.read_u8()?;
            NodeContent::List(Vec::new())
        }
        LIST_EMPTY => NodeContent::List(Vec::new()),
        BINARY_8 => {
            let mut buffer = vec![0u8; stream.read_u8()? as usize];
            stream.read_exact(&mut buffer)?;
            String::from_utf8(buffer).map(|string| NodeContent::String(string.cow())).unwrap_or_else(|err| NodeContent::Binary(err.into_bytes()))
        }
        BINARY_20 => {
            let len: usize = ((stream.read_u8()? as usize & 0x0F) << 16) | (stream.read_u8()? as usize) << 8 | stream.read_u8()? as usize;

            let mut buffer = vec![0u8; len];
            stream.read_exact(&mut buffer)?;
            String::from_utf8(buffer).map(|string| NodeContent::String(string.cow())).unwrap_or_else(|err| NodeContent::Binary(err.into_bytes()))
        }
        BINARY_32 => {
            let mut buffer = vec![0u8; stream.read_u32::<LittleEndian>()? as usize];
            stream.read_exact(&mut buffer)?;
            String::from_utf8(buffer).map(|string| NodeContent::String(string.cow())).unwrap_or_else(|err| NodeContent::Binary(err.into_bytes()))
        }
        JID_PAIR => {
            NodeContent::Jid(Jid::from_node_pair(read_node_content(stream.read_u8()?, stream)?.as_str(), read_node_content(stream.read_u8()?, stream)?.as_str()))
        }
        NIBBLE_8 | HEX_8 => {
            let startbyte = stream.read_u8()?;
            let mut string = String::with_capacity((startbyte as usize & 127) * 2);

            for _ in 0..(startbyte & 127) {
                let byte = stream.read_u8()?;
                if tag == HEX_8 {
                    string.push(char::from_digit(((byte >> 4) & 0x0F) as u32, 16).unwrap().to_ascii_uppercase());
                    string.push(char::from_digit((byte & 0x0F) as u32, 16).unwrap().to_ascii_uppercase());
                } else {
                    let mut nibble = nibble_to_char((byte >> 4) & 0x0F);
                    if nibble == '\0' {
                        return Ok(NodeContent::Nibble(string.cow()));
                    }
                    string.push(nibble);

                    nibble = nibble_to_char(byte & 0x0F);
                    if nibble == '\0' {
                        return Ok(NodeContent::Nibble(string.cow()));
                    }
                    string.push(nibble);
                }
            }
            /*
            if startbyte >> 7 == 0 {
                let len = string.len();
                string.split_off(len - 1);
            }*/
            NodeContent::String(string.cow())
        }
        _ => {
            debug!("Invalid");
            NodeContent::None
        }
    })
}

fn write_node_binary(binary: &[u8], stream: &mut Write) -> Result<()> {
    stream.write_u8(BINARY_8)?;
    stream.write_u8(binary.len() as u8)?;
    stream.write(binary)?;
    Ok(())
}

fn write_node_content(content: NodeContent, stream: &mut Write) -> Result<()> {
    match content {
        NodeContent::None => {
            stream.write_u8(LIST_EMPTY)?;
            write_list(Vec::new(), stream)
        }
        NodeContent::List(list) => { write_list(list, stream) }
        NodeContent::String(string) => {
            let string = string.deref();
            if let Some(token) = TOKENS.iter().position(|r| r == &string) {
                stream.write_u8((token + 3) as u8)
            } else {
                write_node_binary(string.deref().as_bytes(), stream)
            }
        }
        NodeContent::Binary(binary) => {
            write_node_binary(&binary, stream)
        }
        NodeContent::Jid(jid) => {
            stream.write_u8(JID_PAIR)?;
            let pair = jid.into_node_pair();
            write_node_content(NodeContent::Nibble(pair.0.cow()), stream)?;
            write_node_content(NodeContent::Token(pair.1), stream)
        }
        NodeContent::Token(ref token) => {
            stream.write_u8((TOKENS.iter().position(|r| r == token).unwrap() + 3) as u8)
        }
        NodeContent::Nibble(string) => {
            let mut len = (string.len() as u8 + 1) / 2;
            stream.write_u8(NIBBLE_8)?;
            stream.write_u8((string.len() as u8 % 2) << 7 | len)?;
            let mut last_nibble = None;
            for cha in string.chars() {
                let nibble = char_to_nibble(cha);
                if let Some(last_nibble) = last_nibble.take() {
                    stream.write_u8(last_nibble << 4 | nibble)?;
                } else {
                    last_nibble = Some(nibble);
                }
            }
            if let Some(last_nibble) = last_nibble {
                stream.write_u8((last_nibble << 4) + 15)?;
            }
            Ok(())
        }
    }
}

impl Node {
    #[inline]
    pub fn new<D: IntoCow>(desc: D, attributes: HashMap<Cow<'static, str>, NodeContent>, content: NodeContent) -> Node {
        Node {
            desc: desc.cow(),
            attributes,
            content
        }
    }

    #[inline]
    pub fn new_empty<D: IntoCow>(desc: D) -> Node {
        Node {
            desc: desc.cow(),
            attributes: HashMap::new(),
            content: NodeContent::None
        }
    }

    pub fn desc<'a>(&'a self) -> &'a str {
        self.desc.deref()
    }

    pub fn take_attribute<K: IntoCow>(&mut self, key: K) -> Option<NodeContent> {
        self.attributes.remove(&key.cow())
    }

    pub fn get_attribute<'a, K: IntoCow>(&'a self, key: K) -> Option<&'a NodeContent> {
        self.attributes.get(&key.cow())
    }

    pub fn set_attribute<K: IntoCow>(&mut self, key: K, value: NodeContent) {
        self.attributes.insert(key.cow(), value);
    }


    pub fn deserialize(data: &[u8]) -> Result<Node> {
        Node::deserialize_stream(&mut Cursor::new(data))
    }

    fn deserialize_stream(stream: &mut Read) -> Result<Node> {
        let list_size = read_list_size(stream.read_u8()?, stream)?;
        let desc = read_node_content(stream.read_u8()?, stream)?.into_cow();

        let mut attributes = HashMap::new();

        for _ in 0..((list_size - 2 + list_size % 2) / 2) {
            attributes.insert(read_node_content(stream.read_u8()?, stream)?.into_cow(), read_node_content(stream.read_u8()?, stream)?);
        }

        if list_size % 2 == 1 {
            Ok(Node { desc, attributes, content: NodeContent::None })
        } else {
            let tag = stream.read_u8()?;
            Ok(Node {
                desc,
                attributes,
                content: match tag {
                    LIST_EMPTY | LIST_8 | LIST_16 => NodeContent::List(read_list(tag, stream)?),
                    BINARY_8 => {
                        let mut buffer = vec![0u8; stream.read_u8()? as usize];
                        stream.read_exact(&mut buffer)?;
                        NodeContent::Binary(buffer)
                    }
                    BINARY_20 => {
                        let len: usize = ((stream.read_u8()? as usize & 0x0F) << 16) | (stream.read_u8()? as usize) << 8 | stream.read_u8()? as usize;

                        let mut buffer = vec![0u8; len];
                        stream.read_exact(&mut buffer)?;
                        NodeContent::Binary(buffer)
                    }
                    BINARY_32 => {
                        let mut buffer = vec![0u8; stream.read_u32::<LittleEndian>()? as usize];
                        stream.read_exact(&mut buffer)?;
                        NodeContent::Binary(buffer)
                    }
                    _ => unimplemented!()//return Err(())
                }
            })
        }
    }

    pub fn serialize(self) -> Result<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        self.serialize_stream(&mut cursor)?;
        Ok(cursor.into_inner())
    }

    fn serialize_stream(self, stream: &mut Write) -> Result<()> {
        let list_size = match self.content {
            NodeContent::None => 1,
            _ => 2
        } + self.attributes.len() * 2;

        write_list_size(list_size as u16, stream)?;

        write_node_content(NodeContent::String(self.desc), stream)?;

        for attribute in self.attributes {
            write_node_content(NodeContent::String(attribute.0), stream)?;
            write_node_content(attribute.1, stream)?;
        }

        match self.content {
            NodeContent::None => {}
            _ => { write_node_content(self.content, stream)?; }
        }
        Ok(())
    }
}

impl Jid {
    fn from_node_pair(id: &str, surfix: &str) -> Jid {
        Jid {
            id: id.to_string(),
            is_group: match surfix {
                "c.us" => false,
                "g.us" => true,
                "s.whatsapp.net" => false,
                _ => unimplemented!()//return Err(())
            }
        }
    }

    fn into_node_pair(self) -> (String, &'static str) {
        (self.id, if self.is_group {
            "g.us"
        } else {
            "c.us"
        })
    }
}

pub trait IntoCow {
    fn cow(self) -> Cow<'static, str>;
}

impl IntoCow for &'static str {
    fn cow(self) -> Cow<'static, str> {
        Cow::Borrowed(self)
    }
}

impl IntoCow for String {
    fn cow(self) -> Cow<'static, str> {
        Cow::Owned(self.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::Jid;

    #[test]
    fn test_ser_de() {
        let mut attributes = HashMap::new();

        attributes.insert("jid".cow(), NodeContent::Jid(Jid::from_str("12123123-493244232342@g.us").unwrap()));
        attributes.insert("type".cow(), NodeContent::Token("delete"));

        let node = Node::new("action", HashMap::new(), NodeContent::List(vec![Node::new("chat", attributes, NodeContent::None)]));

        let node_ser_de = Node::deserialize(&node.clone().serialize().unwrap()).unwrap();

        assert_eq!(node_ser_de, node);
    }
}
