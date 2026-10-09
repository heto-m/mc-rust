use mcr_buf::{Decode, Encode};

use crate::packet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum Intent {
    Status = 1,
    Login = 2,
    Transfer = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Intention {
    #[mcr(varint)]
    pub protocol_version: i32,
    #[mcr(max_len = 255)]
    pub host: String,
    pub port: u16,
    pub intent: Intent,
}

packet!(Intention, "intention", Serverbound);

#[cfg(test)]
mod tests {
    use mcr_buf::from_bytes_exact;

    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn intention_bytes() {
        let packet = Intention {
            protocol_version: 777,
            host: "localhost".into(),
            port: 25565,
            intent: Intent::Status,
        };
        let mut expected = vec![0x89, 0x06, 9];
        expected.extend(*b"localhost");
        expected.extend([0x63, 0xdd, 1]);
        assert_eq!(roundtrip(&packet), expected);
    }

    #[test]
    fn unknown_intent_rejected() {
        assert!(from_bytes_exact::<Intent>(&[4]).is_err());
        assert!(from_bytes_exact::<Intent>(&[0]).is_err());
    }
}
