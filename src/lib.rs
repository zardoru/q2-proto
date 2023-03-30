pub mod msg_buf;
pub mod netchan;
pub mod objects;
pub mod user_info;

use byteorder::{ReadBytesExt, WriteBytesExt};
use msg_buf::MsgBuf;
use netchan::{NetChan, NetChanVanilla};
use objects::{
    parse_baseline, parse_configstring, parse_print, parse_serverdata, parse_string, DeltaEntity,
    PrintLevel, ServerDataMessage,
};
use std::collections::HashMap;
use std::io::{Cursor, ErrorKind, Write};
use std::net::UdpSocket;
use std::time::{Duration, Instant};
use user_info::UserInfo;

#[derive(PartialEq, Debug)]
pub enum ProtocolVersion {
    Vanilla = 34,
    R1Q2 = 35,
    Q2Pro = 36,
}

pub enum ClientToServerOps {
    Bad,
    Nop,
    Move,
    Userinfo,
    StringCmd,

    //r1q2
    Setting,

    // q2pro
    MoveNodelta = 10,
    MoveBatched,
    UserinfoDelta,
}

const MAX_WRITEABLE_SIZE: usize = 4096;
const MAX_NET_STRING: usize = 2048;
const OOB_PREFIX: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

#[allow(dead_code)]
pub struct Challenge {
    ch_value: String,
    protocols: String,
}

#[derive(Eq, Hash, PartialEq, Clone, Debug)]
pub enum ServerToClientOps {
    Bad = 0,

    // known to all
    MuzzleFlash = 1,
    MuzzleFlash2 = 2,
    TempEntity = 3,
    Layout = 4,
    Inventory = 5,

    // private to client & server
    Nop = 6,
    Disconnect = 7,
    Reconnect = 8,
    Sound = 9,
    Print = 10,
    StuffText = 11,

    ServerData = 12,
    ConfigString = 13,
    SpawnBaseline = 14,
    CenterPrint = 15,
    Download = 16,
    PlayerInfo = 17,
    PacketEntities = 18,
    DeltaPacketEntities = 19,
    Frame = 20,

    // r1q2
    ZPacket = 21,
    ZDownload = 22,
    Gamestate = 23,
    Setting = 24,

    Invalid = -1,
}

impl From<u8> for ServerToClientOps {
    fn from(nm: u8) -> Self {
        let cmd = nm; // (nm >> 5) & ((1 << 5) - 1);
        match cmd {
            1 => ServerToClientOps::MuzzleFlash,
            2 => ServerToClientOps::MuzzleFlash2,
            3 => ServerToClientOps::TempEntity,
            4 => ServerToClientOps::Layout,
            5 => ServerToClientOps::Inventory,
            6 => ServerToClientOps::Nop,
            7 => ServerToClientOps::Disconnect,
            8 => ServerToClientOps::Reconnect,
            9 => ServerToClientOps::Sound,
            10 => ServerToClientOps::Print,
            11 => ServerToClientOps::StuffText,
            12 => ServerToClientOps::ServerData,
            13 => ServerToClientOps::ConfigString,
            14 => ServerToClientOps::SpawnBaseline,
            15 => ServerToClientOps::CenterPrint,
            16 => ServerToClientOps::Download,
            17 => ServerToClientOps::PlayerInfo,
            18 => ServerToClientOps::PacketEntities,
            19 => ServerToClientOps::DeltaPacketEntities,
            20 => ServerToClientOps::Frame,
            21 => ServerToClientOps::ZPacket,
            22 => ServerToClientOps::ZDownload,
            23 => ServerToClientOps::Gamestate,
            24 => ServerToClientOps::Setting,
            _ => ServerToClientOps::Invalid,
        }
    }
}

pub enum ClientEvent {
    Disconnect,
    Reconnect,
    Print(PrintLevel, Vec<u8>),
    StuffText(Vec<u8>),
    CenterPrint(Vec<u8>),
    ServerData(ServerDataMessage),
    ConfigString(u16, Vec<u8>),
    DeltaEntity(DeltaEntity),
}

type ClientEventListener = fn(&ClientEvent);

pub struct Q2ProtoClient {
    socket: UdpSocket,
    server_address: String,
    port: u16,
    connected: bool,
    chan: Box<NetChanVanilla>,
    events: HashMap<ServerToClientOps, Vec<ClientEventListener>>,
    version: String,
    last_precache_value: u32,
    last_msg_sent_time: Instant,
}

impl Q2ProtoClient {
    pub fn new(server: &str, bind_addr: &str, port: u16, version: &str) -> Option<Q2ProtoClient> {
        let socket_opt = UdpSocket::bind(format!("{}:{}", bind_addr, port));
        let socket= match socket_opt {
            Ok(s) => s,
            _ => {
                return None;
            }
        };

        Some(Q2ProtoClient {
            socket,
            server_address: server.to_owned(),
            port,
            connected: false,
            chan: Box::new(NetChanVanilla::new(true, port)),
            events: HashMap::new(),
            version: version.to_string(),
            last_precache_value: 0,
            last_msg_sent_time: Instant::now(),
        })
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn set_read_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        self.socket.set_read_timeout(Some(timeout))
    }

    fn oob_print(&self, msg: &[u8]) -> std::io::Result<usize> {
        let mut send = Vec::with_capacity(4 + msg.len());
        send.extend_from_slice(OOB_PREFIX.as_slice());
        send.extend_from_slice(msg);
        self.socket.send_to(&send, &self.server_address)
    }

    fn recv_connectionless(&self) -> Option<String> {
        let mut buf = [0u8; 1500];
        let recv_bytes = if self.connected {
            self.socket.recv(&mut buf).ok()?
        } else {
            let (bytes, _addr) = self.socket.recv_from(&mut buf).ok()?;
            if _addr != self.server_address.parse().unwrap() {
                return None; // not our server...
            }

            bytes
        };

        if buf[..4] != OOB_PREFIX {
            return None; // not connectionless
        }

        String::from_utf8(buf[4..recv_bytes].to_vec()).ok()
    }

    pub fn status(&self) -> Option<String> {
        self.oob_print(b"status").ok()?;
        self.recv_connectionless()
    }

    pub fn challenge(&self) -> Option<Challenge> {
        self.oob_print(b"getchallenge").ok()?;

        // we're good. skip the prefix and return the challenge
        let str = self.recv_connectionless()?;
        let mut split_pat = str.split(' ');
        if split_pat.next() != Some("challenge") {
            return None;
        };

        let ch_value: &str = split_pat.next()?;
        let protos: &str = split_pat.next()?;

        if !protos.starts_with("p=") {
            return None;
        }

        Some(Challenge {
            ch_value: String::from(ch_value),
            protocols: String::from(&protos[2..]),
        })
    }

    pub fn send_command(&mut self, cmd: &str) -> Option<()> {
        if !self.connected {
            return None;
        }

        self.chan
            .message
            .cur
            .write_u8(ClientToServerOps::StringCmd as u8)
            .ok()?;
        self.chan.message.write_string(cmd)?;

        Some(())
    }

    pub fn connect(
        &mut self,
        challenge: Challenge,
        proto: ProtocolVersion,
        userinfo: UserInfo,
    ) -> Option<()> {
        // woops it takes more work than this to get r1q2 and q2pro support!
        self.last_msg_sent_time = Instant::now();
        assert_eq!(proto, ProtocolVersion::Vanilla);

        // send the connect message
        let msg = format!(
            "connect {} {} {} \"{}\"\n",
            proto as u8,
            self.port,
            challenge.ch_value,
            userinfo.as_string()
        );

        self.oob_print(msg.as_ref()).ok()?;

        self.socket.connect(&self.server_address).ok()?;
        self.connected = true; // we did it! we're considered to be 'connected'.

        self.parse_client_connect();

        self.send_command("new");

        Some(())
    }

    fn parse_command<T: AsRef<[u8]>>(
        &mut self,
        cursor: &mut Cursor<T>,
    ) -> Result<Vec<ClientEvent>, std::io::Error> {
        let mut evts = vec![];

        loop {
            let cmd_val = cursor.read_u8();
            if cmd_val.is_err() {
                break;
            }

            let cmd = ServerToClientOps::from(cmd_val.unwrap());

            let op: Option<ClientEvent> = match cmd {
                ServerToClientOps::Bad => {
                    return Err(std::io::Error::from(ErrorKind::InvalidInput));
                }
                ServerToClientOps::MuzzleFlash => None,
                ServerToClientOps::MuzzleFlash2 => None,
                ServerToClientOps::TempEntity => None,
                ServerToClientOps::Layout => None,
                ServerToClientOps::Inventory => None,
                ServerToClientOps::Nop => None,
                ServerToClientOps::Disconnect => {
                    println!("DISCONNECT BYTE RECV");
                    self.send_command("disconnect");
                    Some(ClientEvent::Disconnect)
                }
                ServerToClientOps::Reconnect => {
                    println!("RECONNECT BYTE RECV");
                    self.send_command("disconnect");
                    Some(ClientEvent::Reconnect)
                }
                ServerToClientOps::Sound => None,
                ServerToClientOps::Print => parse_print(cursor),
                ServerToClientOps::StuffText => {
                    // If we receive a \177c (7f6c -- a short) we need to reply with a command
                    // containing whatever value it requested of us.
                    let str = parse_string(cursor);
                    if self.check_stuffcmd(&str) {
                        Some(ClientEvent::StuffText(str))
                    } else {
                        None
                    }
                }
                ServerToClientOps::ServerData => parse_serverdata(cursor),
                ServerToClientOps::ConfigString => parse_configstring(cursor),
                ServerToClientOps::SpawnBaseline => parse_baseline(cursor),
                ServerToClientOps::CenterPrint => {
                    Some(ClientEvent::CenterPrint(parse_string(cursor)))
                }
                ServerToClientOps::Download => None,
                ServerToClientOps::PlayerInfo => {
                    None // this should be included in Frame
                }
                ServerToClientOps::PacketEntities => None,
                ServerToClientOps::DeltaPacketEntities => None,
                ServerToClientOps::Frame => None,
                ServerToClientOps::ZPacket => None,
                ServerToClientOps::ZDownload => None,
                ServerToClientOps::Gamestate => None,
                ServerToClientOps::Setting => None,
                ServerToClientOps::Invalid => None,
            };

            if let Some(unwrapped_op) = op {
                let vec_listeners = self.events.get(&cmd);

                if let Some(listeners) = vec_listeners {
                    for item in listeners {
                        item(&unwrapped_op);
                    }
                }

                evts.push(unwrapped_op)
            } else {
                // println!("UNABLE TO PARSE: ServerToClientOps::{:?}", cmd);
                break;
            }
        }

        Ok(evts)
    }

    fn parse_client_connect(&mut self) -> Option<()> {
        let data = self.recv_connectionless()?;
        let mut response = data.split(' ');

        if response.next() != Some("client_connect") {
            return None;
        }

        for re in response {
            if re.starts_with("ac=") {
                // anticheat
                self.connected = false;
                return None;
            } 
            // else if re.starts_with("map=") { // map
            // } else if re.starts_with("nc=") { // netchan
            // }
        }

        Some(())
    }

    // do the whole process to get into a server.
    pub fn negotiate(&mut self, proto: ProtocolVersion, userinfo: UserInfo) -> Option<()> {
        let ch = self.challenge()?;
        self.connect(ch, proto, userinfo);

        Some(())
    }

    pub fn subscribe(&mut self, evt: ServerToClientOps, callback: ClientEventListener) {
        self.events.entry(evt.clone()).or_default();
        self.events.get_mut(&evt).unwrap().push(callback);
    }

    pub fn pump(&mut self) -> Result<(), std::io::Error> {
        if !self.connected {
            return Err(std::io::Error::from(ErrorKind::NotConnected));
        }

        let mut buf = [0u8; MAX_WRITEABLE_SIZE];

        while self.socket.peek(&mut buf).is_ok() {
            let res = self.socket.recv(&mut buf)?;
            let mut cur = Cursor::new(&buf[..res]);

            // println!("RECV");
            // hexdump::hexdump(&buf[..res]);

            if self.chan.process(&mut cur) {
                self.parse_command(&mut cur)?;
            }

            let should_nop = self.last_msg_sent_time.elapsed() > Duration::from_secs(2);

            if should_nop {
                self.send_nop();
                self.last_msg_sent_time = Instant::now();
            }

            let data = [0u8; 0];
            if self.chan.should_transmit() {
                let transmit_cursor = self.chan.transmit(&data);
                let transmit_data_size = transmit_cursor.position() as usize;
                let transmit_data = &transmit_cursor.into_inner()[..transmit_data_size];

                // println!("SENT");
                // hexdump::hexdump(&transmit_data);
                self.socket.send(transmit_data)?;
                self.last_msg_sent_time = Instant::now();
            }
        }

        Ok(())
    }

    fn send_nop(&mut self) -> Option<()> {
        self.chan
            .message
            .cur
            .write_u8(ClientToServerOps::Nop as u8)
            .ok()
    }

    fn check_stuffcmd(&mut self, stuff_text: &[u8]) -> bool {
        let cmd_list = stuff_text.split(|f| *f == b'\n');

        for cmd in cmd_list {
            let stuffcmd_head = b"cmd \x7fc";
            let bytes: &[u8] = cmd;

            // Let the protocol (us) handle it.
            // The way Q2 does is by actually expanding the variables but we do the minimum work possible.
            if bytes.starts_with(stuffcmd_head) {
                let cmd_slice = &bytes[7..];
                let cmd_str_opt = String::from_utf8(cmd_slice.to_vec());
                if cmd_str_opt.is_err() {
                    return false;
                }

                let cmd_str = cmd_str_opt.unwrap();
                println!("cmd: {cmd_str}");
                if cmd_str.starts_with("version") {
                    self.send_result_command(format!("version \"{}\"", &self.version).as_ref());
                } else if cmd_str.starts_with("actoken") {
                    self.send_result_command("actoken");
                }
            }

            let changing_cmd = b"changing";
            let precache_cmd = b"precache";

            if bytes.starts_with(precache_cmd) {
                // cmd_precache_f
                // throw an event that requests a precache?
                self.last_precache_value =
                    String::from_utf8(bytes[9..].to_vec()).map_or(0, |f| f.parse().unwrap_or(0));

                let msg = format!("begin {}", self.last_precache_value);
                self.send_command(msg.as_ref());

                self.last_msg_sent_time = Instant::now();
            } else if bytes.starts_with(changing_cmd) {
                // cmd_changing_f
            }
        }

        true // Pass it to the client
    }

    fn send_result_command(&mut self, cmd: &str) -> Option<()> {
        if !self.connected {
            return None;
        }

        self.chan
            .message
            .cur
            .write_u8(ClientToServerOps::StringCmd as u8)
            .ok()?;
        self.chan.message.cur.write_all(b"\x7fc ").ok()?;
        self.chan.message.write_string(cmd)?;

        Some(())
    }
}
