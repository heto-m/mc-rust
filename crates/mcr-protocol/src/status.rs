use mcr_buf::{Decode, Encode};

use crate::packet;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StatusResponse {
    #[mcr(max_len = 32767)]
    pub json: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct PongResponse {
    pub time: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct StatusRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct PingRequest {
    pub time: i64,
}

packet!(StatusResponse, "status_response", Clientbound);
packet!(PongResponse, "pong_response", Clientbound);
packet!(StatusRequest, "status_request", Serverbound);
packet!(PingRequest, "ping_request", Serverbound);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::roundtrip;

    #[test]
    fn status_packets() {
        assert_eq!(roundtrip(&StatusRequest), Vec::<u8>::new());
        assert_eq!(
            roundtrip(&PingRequest { time: 258 }),
            [0, 0, 0, 0, 0, 0, 1, 2]
        );
        assert_eq!(roundtrip(&PongResponse { time: -1 }), [0xff; 8]);
        assert_eq!(
            roundtrip(&StatusResponse { json: "{}".into() }),
            [2, b'{', b'}']
        );
    }
}
