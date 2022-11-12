use std::io::{Cursor, Seek, Write};
use byteorder::{WriteBytesExt};

pub  struct MsgBuf {
    pub cur: Cursor<Vec<u8>>
}

impl MsgBuf {
    pub fn new(size: usize) -> MsgBuf {
        MsgBuf {
            cur: Cursor::new(Vec::with_capacity(size))
        }
    }

    // trim this message
    pub fn get_msg(&self) -> Vec<u8> {
        let  data = self.cur.get_ref();
        let  end = self.cur.position() as usize;

        data[..end].to_vec()
    }

    pub fn clear(&mut self) -> std::io::Result<()> {
        self.cur.rewind()
    }

    // basically an extension that will trim the extra bytes
    // pub fn into_inner(self) -> Vec<u8> {
    //     let  end = self.cur.position() as usize;
    //     let data = self.cur.into_inner();
    //
    //     data[..end].to_vec()
    // }

    pub fn write_string(&mut self, str: &str) -> Option<()> {
        if str.len() > super::MAX_NET_STRING {
            // overflow :(
            self.cur.write_u8(0).ok()?;
            return None
        }

        self.cur.write_all(str.as_bytes()).ok()?;
        self.cur.write_u8(0).ok()?; // null term

        Some(())
    }
}