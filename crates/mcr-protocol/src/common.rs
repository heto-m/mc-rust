//! Packets that exist in both configuration and play (and cookies in login).

use mcr_buf::{
    ByteArray, Decode, DecodeError, Encode, IdOr, Identifier, LengthPrefixed, RestBytes, Result,
    read_len, read_varint, take, write_varint,
};
use mcr_nbt::{Limits, NetworkNbt, OptionalNetworkNbt, Value, write_network};
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

pub(crate) const MAX_CLIENTBOUND_PAYLOAD: usize = 1048576;

#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct ClientboundCustomPayload<'a> {
    pub channel: Identifier,
    pub data: RestBytes<'a>,
}

impl<'a> Decode<'a> for ClientboundCustomPayload<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        Ok(Self {
            channel: Identifier::decode(input)?,
            data: decode_rest_bytes(input, MAX_CLIENTBOUND_PAYLOAD)?,
        })
    }
}

pub(crate) fn decode_rest_bytes<'a>(input: &mut &'a [u8], max: usize) -> Result<RestBytes<'a>> {
    if input.len() > max {
        return Err(DecodeError::TooLong {
            len: input.len(),
            max,
        });
    }
    RestBytes::decode(input)
}

pub(crate) const MAX_SERVERBOUND_PAYLOAD: usize = 32767;

#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct ServerboundCustomPayload<'a> {
    pub channel: Identifier,
    pub data: RestBytes<'a>,
}

impl<'a> Decode<'a> for ServerboundCustomPayload<'a> {
    fn decode(input: &mut &'a [u8]) -> Result<Self> {
        Ok(Self {
            channel: Identifier::decode(input)?,
            data: decode_rest_bytes(input, MAX_SERVERBOUND_PAYLOAD)?,
        })
    }
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

/// Root NBT read without the network heap quota, as vanilla's trusted codecs do.
#[derive(Debug, Clone, PartialEq)]
pub struct TrustedNbt(pub Value);

impl Encode for TrustedNbt {
    fn encode(&self, out: &mut Vec<u8>) {
        write_network(out, Some(&self.0));
    }
}

impl Decode<'_> for TrustedNbt {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Ok(Self(NetworkNbt::decode_with(input, Limits::UNLIMITED)?.0))
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ShowDialog {
    pub dialog: IdOr<TrustedNbt>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ConfigShowDialog {
    pub dialog: TrustedNbt,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PostEffects {
    pub post_effects: Vec<Identifier>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Encode)]
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

impl KnownLinkType {
    const ALL: [Self; 10] = [
        Self::BugReport,
        Self::CommunityGuidelines,
        Self::Support,
        Self::Status,
        Self::Feedback,
        Self::Community,
        Self::Website,
        Self::Forums,
        Self::News,
        Self::Announcements,
    ];
}

/// Vanilla maps out-of-range ids to `BugReport` instead of rejecting them.
impl Decode<'_> for KnownLinkType {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let id = read_varint(input)?;
        Ok(usize::try_from(id)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Self::BugReport))
    }
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
        let serverbound = ServerboundCustomPayload {
            channel: id("a:b"),
            data: RestBytes(&[9]),
        };
        assert_eq!(roundtrip_borrowed!(&serverbound), [3, b'a', b':', b'b', 9]);
    }

    #[test]
    fn serverbound_payload_limit() {
        let mut bytes = vec![3, b'a', b':', b'b'];
        bytes.resize(4 + MAX_SERVERBOUND_PAYLOAD, 0);
        assert!(from_bytes_exact::<ServerboundCustomPayload>(&bytes).is_ok());
        bytes.push(0);
        assert_eq!(
            from_bytes_exact::<ServerboundCustomPayload>(&bytes),
            Err(DecodeError::TooLong {
                len: MAX_SERVERBOUND_PAYLOAD + 1,
                max: MAX_SERVERBOUND_PAYLOAD
            })
        );
    }

    #[test]
    fn cookies() {
        roundtrip(&CookieRequest { key: id("a:b") });
        let present = CookieResponse {
            key: id("a:b"),
            payload: Some(ByteArray(&[1, 2])),
        };
        assert_eq!(
            roundtrip_borrowed!(&present),
            [3, b'a', b':', b'b', 1, 2, 1, 2]
        );
        assert_eq!(
            roundtrip_borrowed!(&CookieResponse {
                key: id("a:b"),
                payload: None,
            }),
            [3, b'a', b':', b'b', 0]
        );
        let mut oversized = vec![3, b'a', b':', b'b', 1, 0x81, 0x28];
        oversized.resize(oversized.len() + MAX_COOKIE_PAYLOAD + 1, 0);
        assert!(from_bytes_exact::<CookieResponse>(&oversized).is_err());
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
        let mut expected = [0; 17];
        expected[15] = 1;
        expected[16] = 3;
        assert_eq!(roundtrip(&response), expected);
    }

    #[test]
    fn dialogs() {
        let dialog = TrustedNbt(Value::String("d".into()));
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
        assert!(from_bytes_exact::<ConfigShowDialog>(&[0]).is_err());
        let mut huge = vec![9, 1, 0x00, 0x30, 0x00, 0x00];
        huge.extend(vec![0; 0x30_0000]);
        assert!(from_bytes_exact::<NetworkNbt>(&huge).is_err());
        assert!(from_bytes_exact::<ConfigShowDialog>(&huge).is_ok());
    }

    #[test]
    fn tags_and_effects() {
        let tags = UpdateTags {
            registries: vec![RegistryTags {
                registry: id("a:b"),
                tags: vec![crate::types::registry::TagEntries {
                    name: id("c:d"),
                    ids: crate::types::registry::IdList(vec![1, 300]),
                }],
            }],
        };
        assert_eq!(
            roundtrip(&tags),
            [
                1, 3, b'a', b':', b'b', 1, 3, b'c', b':', b'd', 2, 1, 0xac, 2
            ]
        );
        assert_eq!(
            roundtrip(&PostEffects {
                post_effects: vec![id("a:b")],
            }),
            [1, 3, b'a', b':', b'b']
        );
        assert_eq!(roundtrip(&ResourcePackPop { id: None }), [0]);
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
        let mut expected = vec![2, 1, 6, 9];
        expected.extend(*b"https://a");
        expected.extend([0, 8, 0, 4]);
        expected.extend(*b"Wiki");
        expected.push(9);
        expected.extend(*b"https://b");
        assert_eq!(roundtrip(&links), expected);
        let unknown = from_bytes_exact::<ServerLinkLabel>(&[1, 42]).unwrap();
        assert_eq!(unknown, ServerLinkLabel::Known(KnownLinkType::BugReport));
    }

    #[test]
    fn clientbound_payload_limit() {
        let mut bytes = vec![3, b'a', b':', b'b'];
        bytes.resize(4 + MAX_CLIENTBOUND_PAYLOAD, 0);
        assert!(from_bytes_exact::<ClientboundCustomPayload>(&bytes).is_ok());
        bytes.push(0);
        assert!(from_bytes_exact::<ClientboundCustomPayload>(&bytes).is_err());
    }

    #[test]
    fn resource_pack_push_bytes() {
        let push = ResourcePackPush {
            id: Uuid::from_u128(1),
            url: "u".into(),
            hash: "h".into(),
            required: false,
            prompt: None,
        };
        let mut expected = Uuid::from_u128(1).as_bytes().to_vec();
        expected.extend([1, b'u', 1, b'h', 0, 0]);
        assert_eq!(roundtrip(&push), expected);
        let long_hash = to_bytes(&ResourcePackPush {
            hash: "x".repeat(41),
            ..push
        });
        assert!(from_bytes_exact::<ResourcePackPush>(&long_hash).is_err());
    }

    #[test]
    fn report_details_bytes() {
        let details = CustomReportDetails {
            details: LengthPrefixed(vec![ReportDetail {
                key: "k".into(),
                value: "v".into(),
            }]),
        };
        assert_eq!(roundtrip(&details), [1, 1, b'k', 1, b'v']);
        let too_many = to_bytes(&CustomReportDetails {
            details: LengthPrefixed(vec![details.details.0[0].clone(); 33]),
        });
        assert!(from_bytes_exact::<CustomReportDetails>(&too_many).is_err());
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
        assert_eq!(
            roundtrip(&CustomClickAction {
                id: id("a:b"),
                payload: Some(Value::Int(5)),
            }),
            [3, b'a', b':', b'b', 5, 3, 0, 0, 0, 5]
        );
    }

    #[test]
    fn disconnect_is_nbt_string() {
        let bytes = roundtrip(&Disconnect {
            reason: Component::text("x"),
        });
        assert_eq!(bytes, [8, 0, 1, b'x']);
    }
}
