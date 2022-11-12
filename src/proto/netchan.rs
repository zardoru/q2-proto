use std::io::{Cursor, Seek, Write};
use super::MsgBuf;
use byteorder::{ReadBytesExt, LittleEndian, WriteBytesExt};
use crate::proto::MAX_WRITEABLE_SIZE;

pub trait NetChan {
    // If it returns true, the packet should be used.
    fn process<T: AsRef<[u8]>>(&mut self, cur: &mut Cursor<T>) -> bool;
    fn transmit(&mut self, data: &[u8]) -> Cursor<[u8; MAX_WRITEABLE_SIZE]>;
    fn should_transmit(&self) -> bool;
}

pub struct NetChanVanilla
{
    pub message: MsgBuf,
    incoming_sequence: u32, // unreliable packet number last received
    incoming_acknowledged: u32, // last reliable packet received

    last_sent_reliable_sequence: u32, // "sequence number of last send" == last_reliable_sequence in chan.c

    outgoing_sequence: u32,

    // reliability ack
    incoming_reliable_acknowledged: bool, // not sure what this is yet?
    incoming_reliable_sequence: bool, // Value that follows last reliable sequence received.
    reliable_sequence: bool, // outgoing reliable sequence
    is_reliable_ack_pending: bool,

    is_client: bool,
    qport: u16,

    reliable_buf: Cursor<[u8; MAX_WRITEABLE_SIZE]>
}

impl NetChanVanilla {
    // The port must have been connected.
    pub fn new(is_client: bool, qport: u16) -> NetChanVanilla {
        NetChanVanilla {
            message: MsgBuf::new(MAX_WRITEABLE_SIZE),
            incoming_sequence: 0,
            incoming_acknowledged: 0,
            reliable_sequence: false,
            incoming_reliable_acknowledged: false,
            incoming_reliable_sequence: false,
            last_sent_reliable_sequence: 0,
            outgoing_sequence: 1,
            is_client,
            qport,
            is_reliable_ack_pending: false,
            reliable_buf: Cursor::new([0; MAX_WRITEABLE_SIZE])
        }
    }
}


// old q2/r1q2 netchan
impl NetChan for NetChanVanilla {
    fn process<T: AsRef<[u8]>>(&mut self, cur: &mut Cursor<T>) -> bool {
        let seq_opt = cur.read_u32::<LittleEndian>();
        let seq_ack_opt = cur.read_u32::<LittleEndian>();

        // when you're a server, you gotta read the qport off your client.
        if !self.is_client {
            // if q2pro, read a byte. else:
            let _qport = cur.read_u16::<LittleEndian>();
        }

        if !seq_opt.is_ok() || !seq_ack_opt.is_ok() {
            return false
        }

        let mut seq = seq_opt.unwrap();
        let mut seq_ack = seq_ack_opt.unwrap();

        let is_reliable_message = (seq & 0x80000000) != 0;
        let is_reliable_ack = (seq_ack & 0x80000000u32) != 0;

        seq &= 0x7FFFFFFF;
        seq_ack &= 0x7FFFFFFF;

        if seq <= self.incoming_sequence { return false; }

        self.incoming_reliable_acknowledged = is_reliable_ack;
        if is_reliable_ack == self.reliable_sequence {
            self.reliable_buf.rewind().unwrap();
        }

        self.incoming_sequence = seq;
        self.incoming_acknowledged = seq_ack;

        if is_reliable_message {
            // we need to ACK the reliable message
            self.is_reliable_ack_pending = true;
            self.incoming_reliable_sequence = !self.incoming_reliable_sequence;
        }

        return true;
    }

    fn transmit(&mut self, data: &[u8]) -> Cursor<[u8; MAX_WRITEABLE_SIZE]> {
        let mut should_send_reliable = false;
        if self.incoming_acknowledged > self.last_sent_reliable_sequence &&
            self.incoming_reliable_acknowledged != self.reliable_sequence {
            should_send_reliable = true;
        }

        /* "if there's data on the message buffer move it to the reliable buffer"
         * and then advance the reliable sequence so let know there's a reliable payload
         * in this case, we should send a reliable payload.
         */
        if self.message.cur.position() > 0 && self.reliable_buf.position() == 0 {
            // this is fine since both buffers have the same size, so just unwrap.
            let lim = self.message.cur.position() as usize;
            let msg_slice = self.message.cur.get_ref().as_slice();
            self.reliable_buf.write_all(&msg_slice[..lim]).unwrap();
            self.message.cur.rewind().unwrap();
            should_send_reliable = true;
            self.reliable_sequence = !self.reliable_sequence;
        }

        let mut outgoing_seq = self.outgoing_sequence & 0x7FFFFFFF;
        let mut incoming_seq = self.incoming_sequence & 0x7FFFFFFF;

        // "we contain a reliable payload"
        if should_send_reliable {
            outgoing_seq |= 0x80000000;
        }

        // "we got a message with a 0 in it as the reliable sequence. here's a '1' back."
        // So this "reliable sequence number" a 0 if the one we got is a 0.
        if self.incoming_reliable_sequence {
            incoming_seq |= 0x80000000;
        }

        let mut packet = Cursor::new([0u8; MAX_WRITEABLE_SIZE]);

        // write header
        packet.write_u32::<LittleEndian>(outgoing_seq).unwrap();
        packet.write_u32::<LittleEndian>(incoming_seq).unwrap();

        if self.is_client {
            // if protocol is q2pro: send one byte of qport
            // else:
            packet.write_u16::<LittleEndian>(self.qport).unwrap();
        }

        if should_send_reliable {
            let data_ref = self.reliable_buf.get_ref();
            packet.write_all(&data_ref[..(self.reliable_buf.position() as usize)]).unwrap();
            self.last_sent_reliable_sequence = self.outgoing_sequence;
        }

        // can use this once it stabilizes. until then...
        // if packet.remaining_slice() >= data.len() { }
        if MAX_WRITEABLE_SIZE - (packet.position() as usize) >= data.len()
            && data.len() > 0 {
            packet.write_all(data).unwrap();
        }

        self.outgoing_sequence += 1;
        self.is_reliable_ack_pending = false;

        packet
    }

    fn should_transmit(&self) -> bool {
        self.is_reliable_ack_pending
            || self.message.cur.position() > 0
            || self.reliable_buf.position() > 0
    }
}
