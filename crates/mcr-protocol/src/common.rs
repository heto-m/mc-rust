//! Packets that exist in both configuration and play (and cookies in login).

use mcr_buf::{
    ByteArray, Decode, DecodeError, Encode, IdOr, Identifier, LengthPrefixed, RestBytes, Result,
    read_len, take, write_varint,
};
use mcr_nbt::{Limits, NetworkNbt, OptionalNetworkNbt, Value};
use uuid::Uuid;

use crate::packet;
use crate::types::registry::RegistryTags;
use crate::types::text::Component;

const MAX_COOKIE_PAYLOAD: usize = 5120;
const MAX_CLICK_PAYLOAD: usize = 65536;
const CLICK_NBT_LIMITS: Limits = Limits {
    max_depth: 16,
    max_bytes: 32768,
};

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClientboundCustomPayload<'a> {
    pub channel: Identifier,
    pub data: RestBytes<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ServerboundCustomPayload<'a> {
    pub channel: Identifier,
    pub data: RestBytes<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ClientboundKeepAlive {
    pub id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ServerboundKeepAlive {
    pub id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct Ping {
    pub id: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct Pong {
    pub id: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Disconnect {
    pub reason: Component,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CookieRequest {
    pub key: Identifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreCookie<'a> {
    pub key: Identifier,
    pub payload: ByteArray<'a>,
}

impl Encode for StoreCookie<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.key.encode(out);
        self.payload.encode(out);
    }
}

impl<'a> Decode<'a> for StoreCookie<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        Ok(Self {
            key: Identifier::decode(input)?,
            payload: decode_cookie_payload(input)?,
        })
    }
}

fn decode_cookie_payload<'a>(input: &mut &'a [u8]) -> Result<ByteArray<'a>> {
    let payload = ByteArray::decode(input)?;
    if payload.0.len() > MAX_COOKIE_PAYLOAD {
        return Err(DecodeError::TooLong {
            len: payload.0.len(),
            max: MAX_COOKIE_PAYLOAD,
        });
    }
    Ok(payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieResponse<'a> {
    pub key: Identifier,
    pub payload: Option<ByteArray<'a>>,
}

impl Encode for CookieResponse<'_> {
    fn encode(&self, out: &mut Vec<u8>) {
        self.key.encode(out);
        self.payload.encode(out);
    }
}

impl<'a> Decode<'a> for CookieResponse<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        let key = Identifier::decode(input)?;
        let payload = if bool::decode(input)? {
            Some(decode_cookie_payload(input)?)
        } else {
            None
        };
        Ok(Self { key, payload })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct Transfer {
    pub host: String,
    #[mcr(varint)]
    pub port: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ReportDetail {
    #[mcr(max_len = 128)]
    pub key: String,
    #[mcr(max_len = 4096)]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CustomReportDetails {
    pub details: LengthPrefixed<ReportDetail, 32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Encode, Decode)]
pub struct ClearDialog;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ShowDialog {
    pub dialog: IdOr<NetworkNbt>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ConfigShowDialog {
    pub dialog: NetworkNbt,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PostEffects {
    pub effects: Vec<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ResourcePackPop {
    pub id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ResourcePackPush {
    pub id: Uuid,
    pub url: String,
    #[mcr(max_len = 40)]
    pub hash: String,
    pub required: bool,
    pub prompt: Option<Component>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum ResourcePackAction {
    SuccessfullyLoaded,
    Declined,
    FailedDownload,
    Accepted,
    Downloaded,
    InvalidUrl,
    FailedReload,
    Discarded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct ResourcePackResponse {
    pub id: Uuid,
    pub action: ResourcePackAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct UpdateTags {
    pub registries: Vec<RegistryTags>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum KnownLinkType {
    BugReport,
    CommunityGuidelines,
    Support,
    Status,
    Feedback,
    Community,
    Website,
    Forums,
    News,
    Announcements,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
#[mcr(tag = u8)]
#[repr(u8)]
pub enum ServerLinkLabel {
    Custom(Component) = 0,
    Known(KnownLinkType) = 1,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ServerLink {
    pub label: ServerLinkLabel,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ServerLinks {
    pub links: Vec<ServerLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum ChatVisibility {
    Full,
    System,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum MainHand {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode, Decode)]
pub enum ParticleStatus {
    All,
    Decreased,
    Minimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ClientInformation {
    #[mcr(max_len = 16)]
    pub language: String,
    pub view_distance: i8,
    pub chat_visibility: ChatVisibility,
    pub chat_colors: bool,
    pub model_customisation: u8,
    pub main_hand: MainHand,
    pub text_filtering_enabled: bool,
    pub allows_listing: bool,
    pub particle_status: ParticleStatus,
}

impl Default for ClientInformation {
    fn default() -> Self {
        Self {
            language: "en_us".into(),
            view_distance: 2,
            chat_visibility: ChatVisibility::Full,
            chat_colors: true,
            model_customisation: 0,
            main_hand: MainHand::Right,
            text_filtering_enabled: false,
            allows_listing: false,
            particle_status: ParticleStatus::All,
        }
    }
}

/// The NBT payload is wrapped in a VarInt length prefix of at most 65536 bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomClickAction {
    pub id: Identifier,
    pub payload: Option<Value>,
}

impl Encode for CustomClickAction {
    fn encode(&self, out: &mut Vec<u8>) {
        self.id.encode(out);
        let mut inner = Vec::new();
        OptionalNetworkNbt(self.payload.clone()).encode(&mut inner);
        write_varint(out, inner.len() as i32);
        out.extend_from_slice(&inner);
    }
}

impl Decode<'_> for CustomClickAction {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let id = Identifier::decode(input)?;
        let len = read_len(input, MAX_CLICK_PAYLOAD)?;
        let mut inner = take(input, len)?;
        let payload = OptionalNetworkNbt::decode_with(&mut inner, CLICK_NBT_LIMITS)?.0;
        Ok(Self { id, payload })
    }
}

packet!(ClientboundCustomPayload<'a>, "custom_payload", Clientbound);
packet!(ServerboundCustomPayload<'a>, "custom_payload", Serverbound);
packet!(ClientboundKeepAlive, "keep_alive", Clientbound);
packet!(ServerboundKeepAlive, "keep_alive", Serverbound);
packet!(Ping, "ping", Clientbound);
packet!(Pong, "pong", Serverbound);
packet!(Disconnect, "disconnect", Clientbound);
packet!(CookieRequest, "cookie_request", Clientbound);
packet!(CookieResponse<'a>, "cookie_response", Serverbound);
packet!(StoreCookie<'a>, "store_cookie", Clientbound);
packet!(Transfer, "transfer", Clientbound);
packet!(CustomReportDetails, "custom_report_details", Clientbound);
packet!(ClearDialog, "clear_dialog", Clientbound);
packet!(ShowDialog, "show_dialog", Clientbound);
packet!(ConfigShowDialog, "show_dialog", Clientbound);
packet!(PostEffects, "post_effects", Clientbound);
packet!(ResourcePackPop, "resource_pack_pop", Clientbound);
packet!(ResourcePackPush, "resource_pack_push", Clientbound);
packet!(ResourcePackResponse, "resource_pack", Serverbound);
packet!(UpdateTags, "update_tags", Clientbound);
packet!(ServerLinks, "server_links", Clientbound);
packet!(ClientInformation, "client_information", Serverbound);
packet!(CustomClickAction, "custom_click_action", Serverbound);

#[cfg(test)]
macro_rules! roundtrip_borrowed {
    ($value:expr) => {{
        fn decode_like<'a, T: ::mcr_buf::Decode<'a>>(_witness: &T, bytes: &'a [u8]) -> T {
            ::mcr_buf::from_bytes_exact(bytes).unwrap()
        }
        let value = $value;
        let bytes = ::mcr_buf::to_bytes(value);
        assert_eq!(&decode_like(value, &bytes), value);
        bytes
    }};
}

#[cfg(test)]
pub(crate) use roundtrip_borrowed;

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;
    use crate::test_util::roundtrip;

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    #[test]
    fn keep_alive_and_ping_bytes() {
        assert_eq!(
            roundtrip(&ClientboundKeepAlive { id: 1 }),
            [0, 0, 0, 0, 0, 0, 0, 1]
        );
        assert_eq!(roundtrip(&ServerboundKeepAlive { id: -1 }), [0xff; 8]);
        assert_eq!(roundtrip(&Ping { id: 2 }), [0, 0, 0, 2]);
        assert_eq!(roundtrip(&Pong { id: 2 }), [0, 0, 0, 2]);
    }

    #[test]
    fn custom_payload_is_rest_bytes() {
        let packet = ClientboundCustomPayload {
            channel: id("minecraft:brand"),
            data: RestBytes(&[7, b'v', b'a', b'n']),
        };
        let bytes = roundtrip_borrowed!(&packet);
        assert_eq!(&bytes[bytes.len() - 4..], [7, b'v', b'a', b'n']);
        roundtrip_borrowed!(&ServerboundCustomPayload {
            channel: id("a:b"),
            data: RestBytes(&[]),
        });
    }

    #[test]
    fn cookies() {
        roundtrip(&CookieRequest { key: id("a:b") });
        let present = CookieResponse {
            key: id("a:b"),
            payload: Some(ByteArray(&[1, 2])),
        };
        let bytes = roundtrip_borrowed!(&present);
        assert_eq!(&bytes[bytes.len() - 4..], [1, 2, 1, 2]);
        roundtrip_borrowed!(&CookieResponse {
            key: id("a:b"),
            payload: None,
        });
        let big = vec![0u8; MAX_COOKIE_PAYLOAD + 1];
        let bytes = to_bytes(&StoreCookie {
            key: id("a:b"),
            payload: ByteArray(&big),
        });
        assert!(from_bytes_exact::<StoreCookie>(&bytes).is_err());
    }

    #[test]
    fn transfer_and_report_details() {
        assert_eq!(
            roundtrip(&Transfer {
                host: "h".into(),
                port: 25565
            }),
            [1, b'h', 0xdd, 0xc7, 1]
        );
        roundtrip(&CustomReportDetails {
            details: LengthPrefixed(vec![ReportDetail {
                key: "k".into(),
                value: "v".into(),
            }]),
        });
    }

    #[test]
    fn resource_packs() {
        roundtrip(&ResourcePackPop { id: None });
        roundtrip(&ResourcePackPop {
            id: Some(Uuid::from_u128(3)),
        });
        let push = ResourcePackPush {
            id: Uuid::from_u128(1),
            url: "http://x".into(),
            hash: "abc".into(),
            required: true,
            prompt: Some(Component::text("hi")),
        };
        roundtrip(&push);
        let response = ResourcePackResponse {
            id: Uuid::from_u128(1),
            action: ResourcePackAction::Accepted,
        };
        let bytes = roundtrip(&response);
        assert_eq!(bytes[16], 3);
    }

    #[test]
    fn dialogs() {
        let dialog = NetworkNbt(Value::String("d".into()));
        assert_eq!(
            roundtrip(&ConfigShowDialog {
                dialog: dialog.clone()
            }),
            [8, 0, 1, b'd']
        );
        assert_eq!(
            roundtrip(&ShowDialog {
                dialog: IdOr::Inline(dialog)
            }),
            [0, 8, 0, 1, b'd']
        );
        assert_eq!(
            roundtrip(&ShowDialog {
                dialog: IdOr::Id(2)
            }),
            [3]
        );
        assert_eq!(roundtrip(&ClearDialog), Vec::<u8>::new());
    }

    #[test]
    fn tags_and_effects() {
        roundtrip(&UpdateTags {
            registries: vec![RegistryTags {
                registry: id("block"),
                tags: vec![],
            }],
        });
        roundtrip(&PostEffects {
            effects: vec![id("a:b")],
        });
    }

    #[test]
    fn server_links() {
        let links = ServerLinks {
            links: vec![
                ServerLink {
                    label: ServerLinkLabel::Known(KnownLinkType::Website),
                    url: "https://a".into(),
                },
                ServerLink {
                    label: ServerLinkLabel::Custom(Component::text("Wiki")),
                    url: "https://b".into(),
                },
            ],
        };
        let bytes = roundtrip(&links);
        assert_eq!(&bytes[..3], [2, 1, 6]);
    }

    #[test]
    fn client_information_bytes() {
        let bytes = roundtrip(&ClientInformation::default());
        assert_eq!(
            bytes,
            [5, b'e', b'n', b'_', b'u', b's', 2, 0, 1, 0, 1, 0, 0, 0]
        );
    }

    #[test]
    fn click_action() {
        let none = CustomClickAction {
            id: id("a:b"),
            payload: None,
        };
        assert_eq!(roundtrip(&none), [3, b'a', b':', b'b', 1, 0]);
        roundtrip(&CustomClickAction {
            id: id("a:b"),
            payload: Some(Value::Int(5)),
        });
    }

    #[test]
    fn disconnect_is_nbt_string() {
        let bytes = roundtrip(&Disconnect {
            reason: Component::text("x"),
        });
        assert_eq!(bytes, [8, 0, 1, b'x']);
    }
}
