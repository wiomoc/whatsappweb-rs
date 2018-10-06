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
    degrees_latitude: f64,
    degrees_longitude: f64,
    name: String,
    address: String,
    url: String,
    jpeg_thumbnail: Vec<u8>,
}

#[derive(Debug)]
pub struct LiveLocationMessage {
    // message fields
    degrees_latitude: f64,
    degrees_longitude: f64,
    accuracy_in_meters: u32,
    speed_in_mps: f32,
    degrees_clockwise_from_magnetic_north: u32,
    caption: String,
    sequence_number: i64,
    jpeg_thumbnail: Vec<u8>,
}

#[derive(Debug)]
pub enum ChatMessageContent {
    Text(String),
    Image(FileInfo, (u32, u32), Vec<u8>),
    Audio(FileInfo, Duration),
    Document(FileInfo, String),
    Location(LocationMessage),
    LiveLocation(LiveLocationMessage),

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
                //liveLocationMessage {degreesLatitude: 28.1039651 degreesLongitude: -15.41814 caption: "Test" sequenceNumber: 1538811189000001 jpegThumbnail: "\377\330\377\340\000\020JFIF\000\001\001\000\000\001\000\001\000\000\377\333\000C\000\006\004\005\006\005\004\006\006\005\006\007\007\006\010\n\020\n\n\t\t\n\024\016\017\014\020\027\024\030\030\027\024\026\026\032\035%\037\032\033#\034\026\026 , #&')*)\031\037-0-(0%()(\377\333\000C\001\007\007\007\n\010\n\023\n\n\023(\032\026\032((((((((((((((((((((((((((((((((((((((((((((((((((\377\300\000\021\010\000d\000d\003\001\"\000\002\021\001\003\021\001\377\304\000\034\000\000\001\005\001\001\001\000\000\000\000\000\000\000\000\000\000\000\001\002\003\004\005\007\010\006\377\304\000=\020\000\002\001\003\002\003\006\003\006\004\003\t\000\000\000\000\001\002\003\000\004\021\022!\0051A\023\"2Qaq\006\201\221\024#B\241\301\321\0073R\360\025\026\261$4Cbr\202\262\341\361\377\304\000\032\001\001\000\002\003\001\000\000\000\000\000\000\000\000\000\000\000\000\001\002\003\004\005\006\377\304\000,\021\000\002\002\002\000\003\006\005\005\000\000\000\000\000\000\000\000\001\002\021\003!\004\0221\005\006\023AQ\360\"q\221\301\341r\241\261\321\361\377\332\000\014\003\001\000\002\021\003\021\000?\000\364\265\254\275\224\233\370N\307\323\326\246\276\213\376*\3737\357U\346\031!\307'\031\371\365\253Vr\t#1\276\344\016\275E\001D\022\010 \340\215\301\253[\311\211#Ru\370\200\350\324\331\"\212\006\373\302\316N\341F\333z\232#\234\263vl\025#a\244\005\333MCW\246\023\241\341W\032dmN3\204C\371f\223,\313\201\204O\351O\336\221F\001^L\247\033y\371\323n&\212\336&\232y\022(\207\211\235\202\252\237sX\334\235h\272[\036\240(\300\000\nZ\316\260\343\\3\210\316a\341\334F\312\352Q\315b\270V\307\320\325\376\347V2\237%\331~\265E\033\331y\\]4,`H\014{\222\235\345#\247\247\367\372R)\324\025\200\013\260\306\331?SJY\212\340\220\212?\nl\007\316\224\356\232\212\214\347\031=}qY.\326\231\216\253\250\323\202\333\345\233\352iy0\325\2009\020N\344{Q\2760I\307\220\330P\000\034\206*\234\311;-L9\034\023\313\257\235\024\204\3040%\014H\345\217**\334\211\355\021\315DAX\304W\035\364c\250u\033\nlnQ\303\2571\371\325\202D\254\241\233L\270\314r\017\304?\276\225\033\251b\301\227L\2522@\344\303\316\262\224-\\ \236\000\351\271\003#\366\254\343\202=*\335\214\272[\263c\261\334{\322\334\252@\332\3260\314\307\233r\007\332\200\216Y\002\331\265\324\254#X\220\231\035\366\032@\316\252\363\247\305\337\022\335\361\376\"\363\334\310\311\002\022b\210\234,C\333\317\314\373\364\300\035\353\216\307=\377\000\006\3426\252\347]\315\264\260\r\360\006\245 \037\316\274\313\177\013:M\027yX\355\206\030#\320\3277\264$\325%\321\236\253\273\030q\316S\311%r]\002\013\305\221\276\352\\\262\235_1\326\273\217\360\253\342y\370\315\214\2667\362k\271\266\000\254\254r]9o\324\221\266\376Ds95\347\353\0139b\270\355$\302\201\236\271\315u\317\340\225\224\215\306/\257\267\020\305na>D\273)\037\370\037\250\363\255^\032N9\022[G[\266\361C/\006\362dU%\320\354^\303\1773\317\351G#\223\223\236~dP\016Gtdy\364\372\321\363\317\242\376\365\326\370\272\236\013A\341\330\221\221\371\320s\327\273\377\000W\355\316\227'N\024i t\346G\226hP\t\030\353\326\215$JlM9\351!\365\000\n+\211\336_\177\217^\334__q\244\262fr\022\027Y[J\016X\322\244\001\372\344\365\242\272\213\263\265\267\277\223<\304\273\306\233|\220M~\270\257\333\310\355\330i\355\264\314\025\\\234\247BO\265D\t\270\203\007y\242\372\221\375\377\000\245>d\225\242\016\300v\261\234\202:\212c\276\227K\230\2717\210z\365\025\242z2\000y\020}A\255\025+uo\203\261\344}\rS\270@\254$O\345\311\270\364>T[K\331I\223\341;\037\336\200\215\201V*\303\004lk\217\177\025~\034\373\025\350\342\226\211\213{\206\373\320\007\206Ny\366;\237|\371\212\355\267\321dv\253\323f\366\363\254>=\303\223\213p\213\253)1\367\250B\223\370[\232\237\221\000\374\253\016|K,\034Y\275\331\334l\270,\361\312\272y\374\217:X\332\315{w\r\265\262\031&\225\202\"\216\244\234\017\376\327\243\276\026\340\220\360\016\007\r\224!ZE\357M&<ly\266>\200y\000+\230\377\000\0068ts\361\273\253\351\243 \333F; \335\031\3623\356\002\260\377\000\272\273\"\345\234\004\031#\237\220\036\265\247\301b\344\\\317\253;\035\342\343\236l\276\004_\303\037\344\016\376\"O\275-!\302\234\002\031O\204\216\264r\361\020\276\373\237\245m8\311\263\316\332\002q\276q\216\264\354\020\003\354\240\364#\221\366\244\344\304\r\210\353\314\322`\014\365\007\237\255YR\323#og=\342_\007q88\205\303p_\360\347\263\231\314\240][\304\356\204\363\\\262\223\217-\377\000Z+\241\022\027f#\320\371\217:+mq\331V\232_C\221.\303\341\244\333NJ\374\223\321\215\361_\030\273\340\374\025n\370}\252M0\226(\243\210\261!\365\270\\~{T\034k\3428m\227\203/\rH\356\027\212O\0136\242p\260\273\242\227\030\353\231\023\037?*\226\377\000\200\3334v\342+{Kk\237\264[O5\324q\000f\354\344W \2203\370v\311\254\344\370P\301w/\373X}7\260K\002\024?u\014sv\3061\216\244\226\371\005\362\244y+f\316O\032\337/\232\372\177\246\354\034R\312HaE\231\214Ww/m\001(s\332\241}C\353\033o\351\353T-\370\367\016\235]\373Yb\205a{\2014\2612G$k\215L\254y\201\221\3629\345Q'\000\275\267\270\261F\232\330XX\361\toP\200\306W\355\014\247I\3500e;\357\234t\252\237\345\213\231m\256\222{\213{{[\213i-\344\026\201\302\313#\020D\2463\335B0v^z\216\364\345\207\250s\315Z\217\272_{\367\327r\323\342K\017\262\\\033\217\264Bm\302\023\034\3202\310\312\347Ji\\e\265\035\200\033\347j,om\357\036U\207\264I\"p\222C2\024\221\t\031\031\007\314r=k\016\337\341\351V)\037M\205\265\342\2742B\360\366\216\031\243}]\362\307\302y`r\334\344\327\321p{+\244\236\373\210]\274\006\356\357@\t\016JF\250\010\003'\004\235\333'\003\237-\252$\243Z/\216Y[\\\313\336\377\000\007\310\360Y\033\203\360\251x\214\026\234>+\033\236\"\320\274\010d\0231\023\230\201RX\202F\t\320\000\034\361\212\372H\270\344\027\034e,b:m\013<\n\346\027\323,\353\234\250\177\016\301[<\362A\0357\251\3018\005\217\r\304\317ie/\020\355\346\230\335\010\027\264\357\310\314;\304g`\300|\252\257\t\370tXqT\224\245\234\226\361\334Ip\222\267hf\032\213\020\272s\244\020[\305\3449o\232T6\221^n!\270\271;\272\273w\357\360i[\374Ma=\2247\026\353z\351tt\333\250\266p\363wC\026@w\323\216ga\371Qu\361'\016\265\262\216\361\205\333Z:\t;h\355]\224d\221\202q\263dc\007z\316\272\370w_\n\340\020\253A=\307\013\267\026\345e.\221\3124*\266\353\2709@F\307\313\255+\360K\345^\030\266\255\303E\255\220iM\261Y\002\031\231\211\327\314\223\214\355\236\244\237,G&6<L\352\365\351\366\374\233\034c\210=\2346\311k\022\313ws2\333\300\262\022\253\222\013\035]p\025X\343\323\024\211ywe\r\323\361\250\242X\342(\"\232\330\026\023\026\333HM\3306p:\347#\327\021q{)/\241\2660J\221^\333N\2270\261R\310\\\002\010#\236\010f\036{\326c\3745v\037\210\335\241\260\341\327W\242\0250Y\206U\220$\214\357\255\327\r\227\014A d\017:\205\0105\262\323\226U+\212\367_\337\276\247\321p\253\370\270\2042\265\260u0\310b\2269\242*\361\276\001\301\007\321\201\371\321Y\377\000\017p\353\336\026\267\335\353\030\376\325q\333\366q#\025\217\356\3214\202N\376\016}s\323\225\025F\342\235&e\203\233\215\311l\336d\205\341\354\201\001t\352\003\250\036uJI3p\322\241\374@\203\362\305H\256\025m\344a\225\031\215\275\251\322@\204\262B0\340\002\001l\206Z\222\345\231\214m\230\235\200g\030\025N\331\260\315\014\243\272\375\323\350j\311\215Q\"iF\266B\027W,o\316\253\336\250'X\030%\212\266<\372\032\002\027R\216\310\334\301\372\325\213)t>\206\360\267/CM\271\357\244S\177P\322}\377\000\274\324Q\306\362\234 \310\352N\300|\350\t\357b\320\375\240\360\267?CU\321Y\374#o3\312\264\243d\2262\245\226B6lr\254\351\265\353e\220\347\007\030\351@6H\213\306\302\031s \031\345\261\366\252\334>)\343\220\317!\t\027&.|UgpA\033\021\312\244\220\353\335\3110H4:\237\302|\305c\2265))z\024\224-\246I#\0104\210\007\215s\332\035\311\366\2463x$\346H\322\307\317\024\226\361:\306mf+\254e\2429\350(\217\275\033\251\030#\275\217Q\316\257\325\027L\230\034\214\212*8\263\243lQZ\355S\243*cS\275o2\377\000I\016?Zpm&\322O.\341\371\034Sa\331'o(\361\3634\331?\335\020y\263\037\322\266LE\310\244\005\336\t5\023\222\006z\216t\347\0224\345\n\016\301\206\t\333}\252#\0335\342\272\343H\nK\036\274\352 \361\305#\272\207iI<\366\000\346\200pG\0262+\202\n6G\353\372\323d,\366\320\351\324P\r,\240u\024E\265\224\347\315\200\377\000O\336\221d1\"2\222\006\242H\035F\324\002[K\331J\033=\323\261\366\2537\361l$\03565Vt\3213\201\310\367\207\261\253\226n$\204\306\373\2201\356(\n\024\350\237\263|\237\t\347\373\321*\030\244en\235|\305,qI'\201\t\036ga@%\304r>\226BM\304=\345?\326\26519\321p\024\200\177\230\270\344z\324\261\306\211\033$\222\006`\t\302\363\003\312\230/\002\200#\215\212\377\000\314\333\376\265\025\273\"\210\031YY\225r@=(\253\022\307,\254\036\t>\355\200 d\214QRH\331\302\213L\252\252\352q\220\007\225G0\304v\340r\320[\346qE\024\005\253.\365\262\203\320\340|\215S\n\036\344\251\344\322\020~\246\212(\t\010\305\223\201\313\265\307\320\377\000\352\243\177\345\307\354\177\324\321E\000\262oon\307\236\n\374\201\247p\356\374\245\211\306\234\215\272\321E\001zU\\\027*\245\224lH\254\346\236I\206Y\210\036K\260\242\212\001mv\271\217\035I\037\225G\214\022\007 H\242\212\001\r\324\260wc#I\337qE\024P\037\377\331"}

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