extern crate core;

use crate::proto::{ServerToClientOps, user_info};
use crate::user_info::UserInfo;

mod  proto;


fn main() {
    let mut conn = proto::Q2ProtoClient::new(
        "127.0.0.1:27910",
        27955,
        String::from("q2proto-test v0.1")).expect("?");

    // let txt = conn.status().expect("a");
    // let uinfo = txt.lines().nth(1).expect("no userinfo?");
    // let parsed_uinfo = proto::UserInfo::from_string(uinfo);
    // print!("{:?}", parsed_uinfo.keys);

    let uinfo = UserInfo::from_string("\\name\\asdfasdf");

    conn.negotiate(proto::ProtocolVersion::Vanilla, uinfo);

    conn.subscribe(ServerToClientOps::Print, |evt| {
        if let proto::ClientEvent::Print(lv, msg) = evt {
            let s = String::from_utf8(msg.clone())
                .unwrap_or("<null>".parse().unwrap());
            let s_trim = s.trim();
            println!("MSG: {s_trim}");
        }
    });

    while conn.is_connected() {
        conn.pump().unwrap_or(());
    }
}
