use super::ClientEvent;
use super::ClientEvent::ServerData;
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor};
use std::ops::{BitAnd, BitOr};

pub struct PackedEntity {}

// How one updates the entity depends on these bits.
pub enum EntityStateBits {
    ORIGIN1 = (1 << 0),
    ORIGIN2 = (1 << 1),
    ANGLE2 = (1 << 2),
    ANGLE3 = (1 << 3),
    FRAME8 = (1 << 4),
    // frame is a byte
    EVENT = (1 << 5),
    REMOVE = (1 << 6),
    // REMOVE this entity, don't add it
    MOREBITS1 = (1 << 7),
    // read one additional byte
    NUMBER16 = (1 << 8),
    // NUMBER8 is implicit if not set // second byte
    ORIGIN3 = (1 << 9),
    ANGLE1 = (1 << 10),
    MODEL = (1 << 11),
    RENDERFX8 = (1 << 12),
    // fullbright, etc
    ANGLE16 = (1 << 13),
    EFFECTS8 = (1 << 14),
    // autorotate, trails, etc
    MOREBITS2 = (1 << 15),
    // read one additional byte
    SKIN8 = (1 << 16),
    // third byte
    FRAME16 = (1 << 17),
    // frame is a short
    RENDERFX16 = (1 << 18),
    // 8 + 16 = 32
    EFFECTS16 = (1 << 19),
    // 8 + 16 = 32
    MODEL2 = (1 << 20),
    // weapons, flags, etc
    MODEL3 = (1 << 21),
    MODEL4 = (1 << 22),
    MOREBITS3 = (1 << 23),
    // read one additional byte
    OLDORIGIN = (1 << 24),
    // FIXME: get rid of this // fourth byte
    SKIN16 = (1 << 25),
    SOUND = (1 << 26),
    SOLID = (1 << 27),
}

impl BitAnd<EntityStateBits> for u32 {
    type Output = u32;

    fn bitand(self, rhs: EntityStateBits) -> Self::Output {
        self & (rhs as u32)
    }
}

impl BitOr<EntityStateBits> for EntityStateBits {
    type Output = u32;

    fn bitor(self, rhs: EntityStateBits) -> Self::Output {
        (self as u32) | (rhs as u32)
    }
}

#[derive(Eq, PartialEq, Hash)]
pub enum PrintLevel {
    LOW = 0,
    // pickup messages
    MEDIUM = 1,
    // death messages
    HIGH = 2,
    // critical messages
    CHAT = 3,
    // chat messages
    UNK = -1, // Error?
}

impl From<u8> for PrintLevel {
    fn from(b: u8) -> Self {
        match b {
            0 => PrintLevel::LOW,
            1 => PrintLevel::MEDIUM,
            2 => PrintLevel::HIGH,
            3 => PrintLevel::CHAT,
            _ => PrintLevel::UNK,
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
pub struct R1Q2ProtocolInfo;

#[derive(Eq, Hash, PartialEq)]
pub struct Q2ProProtocolInfo;

#[derive(Eq, Hash, PartialEq)]
pub enum ProtocolInfo {
    Vanilla,
    R1Q2(R1Q2ProtocolInfo),
    Q2Pro(Q2ProProtocolInfo),
}

#[derive(Eq, Hash, PartialEq)]
pub struct ServerDataMessage {
    protocol: u32,
    srv_count: u32,
    attract_loop: u8,
    gamedir: String,
    clnum: u16,
    levelname: String,
    // protocol specific info below
    protocol_info: ProtocolInfo,
}

pub fn parse_string<T: AsRef<[u8]>>(cur: &mut Cursor<T>) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();

    while let Ok(byte) = cur.read_u8() {
        // XXX: real quake 2 breaks with a signed -1 byte.
        if byte == 0 {
            break;
        }

        out.push(byte)
    }

    out
}

// may return characters not printable in the utf8 range, so...
pub fn parse_print<T: AsRef<[u8]>>(cur: &mut Cursor<T>) -> Option<ClientEvent> {
    let level = PrintLevel::from(cur.read_u8().ok()?);
    let content = parse_string(cur);

    Some(ClientEvent::Print(level, content))
}

pub fn parse_serverdata<T: AsRef<[u8]>>(cur: &mut Cursor<T>) -> Option<ClientEvent> {
    Some(ServerData(ServerDataMessage {
        protocol: cur.read_u32::<LittleEndian>().ok()?,
        srv_count: cur.read_u32::<LittleEndian>().ok()?,
        attract_loop: cur.read_u8().ok()?,
        gamedir: String::from_utf8(parse_string(cur)).ok()?,
        clnum: cur.read_u16::<LittleEndian>().ok()?,
        levelname: String::from_utf8(parse_string(cur)).ok()?,
        protocol_info: ProtocolInfo::Vanilla,
    }))
}

pub fn parse_configstring<T: AsRef<[u8]>>(cur: &mut Cursor<T>) -> Option<ClientEvent> {
    Some(ClientEvent::ConfigString(
        cur.read_u16::<LittleEndian>().ok()?,
        parse_string(cur),
    ))
}

// returns number / bits
pub fn parse_entity_bits<T: AsRef<[u8]>>(cur: &mut Cursor<T>) -> Option<(i16, u32)> {
    let mut total: u32 = cur.read_u8().ok()? as u32;
    if total & EntityStateBits::MOREBITS1 != 0 {
        total |= (cur.read_u8().ok()? as u32) << 8;
    }
    if total & EntityStateBits::MOREBITS2 != 0 {
        total |= (cur.read_u8().ok()? as u32) << 16;
    }
    if total & EntityStateBits::MOREBITS3 != 0 {
        total |= (cur.read_u8().ok()? as u32) << 24;
    }

    let number = if total & EntityStateBits::NUMBER16 != 0 {
        cur.read_i16::<LittleEndian>().ok()?
    } else {
        cur.read_i8().ok()? as i16
    };

    Some((number, total))
}

// fields that are not None are fields that changed.
#[allow(dead_code)]
pub struct DeltaEntity {
    number: i16,
    model_index: Option<u8>,
    model_index2: Option<u8>,
    model_index3: Option<u8>,
    model_index4: Option<u8>,
    frame: Option<i16>,
    skin: Option<u32>,
    effects: Option<u32>,
    render_fx: Option<u32>,
    origin0: Option<f32>,
    origin1: Option<f32>,
    origin2: Option<f32>,
    angle0: Option<f32>,
    angle1: Option<f32>,
    angle2: Option<f32>,
    old_origin0: Option<f32>,
    old_origin1: Option<f32>,
    old_origin2: Option<f32>,
    // these are i32 in the q2 source, but only a byte is ever parsed out of a packet
    sound: Option<u8>,
    event: u8,
    solid: Option<u32>,
}

pub fn parse_baseline<T: AsRef<[u8]>>(cur: &mut Cursor<T>) -> Option<ClientEvent> {
    let (number, bits) = parse_entity_bits(cur)?;
    parse_delta_entity(number, bits, cur)
}

fn parse_delta_entity<T: AsRef<[u8]>>(
    entnum: i16,
    bits: u32,
    cur: &mut Cursor<T>,
) -> Option<ClientEvent> {
    Some(ClientEvent::DeltaEntity(DeltaEntity {
        number: entnum,
        model_index: if bits & EntityStateBits::MODEL != 0 {
            Some(cur.read_u8().ok()?)
        } else {
            None
        },
        model_index2: if bits & EntityStateBits::MODEL2 != 0 {
            Some(cur.read_u8().ok()?)
        } else {
            None
        },
        model_index3: if bits & EntityStateBits::MODEL3 != 0 {
            Some(cur.read_u8().ok()?)
        } else {
            None
        },
        model_index4: if bits & EntityStateBits::MODEL4 != 0 {
            Some(cur.read_u8().ok()?)
        } else {
            None
        },
        frame: if bits & EntityStateBits::FRAME8 != 0 && bits & EntityStateBits::FRAME16 != 0 {
            // both are set, read both
            cur.read_u8().ok()?;
            Some(cur.read_i16::<LittleEndian>().ok()?)
        } else if bits & EntityStateBits::FRAME8 != 0 {
            // only F8 is set
            Some(cur.read_u8().ok()?.into())
        } else if bits & EntityStateBits::FRAME16 != 0 {
            // only F16 is set
            Some(cur.read_i16::<LittleEndian>().ok()?)
        } else {
            None
        }, // neither is set
        skin: if bits & (EntityStateBits::SKIN8 | EntityStateBits::SKIN16)
            == (EntityStateBits::SKIN8 | EntityStateBits::SKIN16)
        {
            Some(cur.read_u32::<LittleEndian>().ok()?) // laser
        } else if bits & EntityStateBits::SKIN8 != 0 {
            Some(cur.read_u8().ok()?.into())
        } else if bits & EntityStateBits::SKIN16 != 0 {
            Some(cur.read_u16::<LittleEndian>().ok()? as u32)
        } else {
            None
        },
        effects: if bits & (EntityStateBits::EFFECTS8 | EntityStateBits::EFFECTS16)
            == (EntityStateBits::EFFECTS8 | EntityStateBits::EFFECTS16)
        {
            Some(cur.read_u32::<LittleEndian>().ok()?) // laser
        } else if bits & EntityStateBits::EFFECTS8 != 0 {
            Some(cur.read_u8().ok()?.into())
        } else if bits & EntityStateBits::EFFECTS16 != 0 {
            Some(cur.read_u16::<LittleEndian>().ok()? as u32)
        } else {
            None
        },
        render_fx: if bits & (EntityStateBits::RENDERFX8 | EntityStateBits::RENDERFX16)
            == (EntityStateBits::RENDERFX8 | EntityStateBits::RENDERFX16)
        {
            Some(cur.read_u32::<LittleEndian>().ok()?) // laser
        } else if bits & EntityStateBits::RENDERFX8 != 0 {
            Some(cur.read_u8().ok()?.into())
        } else if bits & EntityStateBits::RENDERFX16 != 0 {
            Some(cur.read_u16::<LittleEndian>().ok()? as u32)
        } else {
            None
        },
        origin0: if bits & EntityStateBits::ORIGIN1 != 0 {
            parse_coord(cur)
        } else {
            None
        },
        origin1: if bits & EntityStateBits::ORIGIN2 != 0 {
            parse_coord(cur)
        } else {
            None
        },
        origin2: if bits & EntityStateBits::ORIGIN3 != 0 {
            parse_coord(cur)
        } else {
            None
        },
        angle0: if bits & EntityStateBits::ANGLE1 != 0 {
            parse_angle(cur)
        } else {
            None
        },
        angle1: if bits & EntityStateBits::ANGLE2 != 0 {
            parse_angle(cur)
        } else {
            None
        },
        angle2: if bits & EntityStateBits::ANGLE3 != 0 {
            parse_angle(cur)
        } else {
            None
        },
        old_origin0: if bits & EntityStateBits::OLDORIGIN != 0 {
            parse_coord(cur)
        } else {
            None
        },
        old_origin1: if bits & EntityStateBits::OLDORIGIN != 0 {
            parse_coord(cur)
        } else {
            None
        },
        old_origin2: if bits & EntityStateBits::OLDORIGIN != 0 {
            parse_coord(cur)
        } else {
            None
        },
        sound: if bits & EntityStateBits::SOUND != 0 {
            cur.read_u8().ok()
        } else {
            None
        },
        event: if bits & EntityStateBits::EVENT != 0 {
            cur.read_u8().ok()?
        } else {
            0
        },
        solid: if bits & EntityStateBits::SOLID != 0 {
            Some(cur.read_u16::<LittleEndian>().ok()?.into())
        } else {
            None
        },
    }))
}

fn parse_angle<T: AsRef<[u8]>>(p0: &mut Cursor<T>) -> Option<f32> {
    Some((p0.read_u8().ok()? as f32) * 360.0 / 256.0)
}

fn parse_coord<T: AsRef<[u8]>>(p0: &mut Cursor<T>) -> Option<f32> {
    Some((p0.read_u16::<LittleEndian>().ok()? as f32) / 8.0)
}

// fn parse_frame<T: AsRef<[u8]>>(entnum: i16, bits: u32, cur: &mut Cursor<T>) -> Option<ClientEvent> {
//     let _currentframe = cur.read_u32::<LittleEndian>().ok()?;
//     let _deltaframe = cur.read_u32::<LittleEndian>().ok()?;
//     let _supressed = cur.read_u8().ok()?; // ?? we don't do anything with this?
//
//     // new protocol will tend to check deltas and whatever. we don't care because we're protocol 34 baby
//     let areabits_len = cur.read_u8().ok()?;
//     let areabits = [0u8; 32];
//     cur.read_exact(&mut areabits[..areabits_len])?;
//
//     // xxx: what are these areabits for?? "portalarea visibility bits" the hell does that mean
//     // has something to do with visibility?? i suppose??
//
//     // parse playerstate
//
//     // parse packetentities
//
//     // deltaframe??
//
//     Some(ClientEvent())
// }
