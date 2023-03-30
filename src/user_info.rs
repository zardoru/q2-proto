use std::collections::HashMap;

pub struct UserInfo {
    pub keys: HashMap<String, String>,
}

impl UserInfo {
    pub fn new() -> UserInfo {
        UserInfo {
            keys: HashMap::new(),
        }
    }

    pub fn from_string(userinfo: &str) -> UserInfo {
        let mut ret = UserInfo {
            keys: HashMap::new(),
        };

        // Shouldn't happen.
        if !userinfo.starts_with('\\') {
            return ret;
        }

        let spl = userinfo.split('\\');
        // skip over the empty userinfo bit and make the info pairs
        let spl_vec: Vec<&str> = spl.skip(1).collect();
        for n in 0usize..(spl_vec.len() / 2) {
            ret.keys.insert(
                spl_vec[n * 2].parse().unwrap(),
                spl_vec[n * 2 + 1].parse().unwrap(),
            );
        }

        ret
    }

    pub fn as_string(&self) -> String {
        let mut kvs: Vec<String> = vec![];
        kvs.reserve(self.keys.len());
        for (key, val) in &self.keys {
            kvs.push(format!("\\{key}\\{val}"))
        }

        kvs.join("")
    }
}

impl Default for UserInfo {
    fn default() -> Self {
        Self::new()
    }
}