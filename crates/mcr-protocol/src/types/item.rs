use mcr_buf::{
    Decode, DecodeError, Encode, LengthPrefixed, Result, VarInt, read_varint, write_varint,
};

use super::component::DataComponentPatch;

/// Optional stack: a count of zero or less is the empty stack and carries nothing else.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemStack {
    pub item: i32,
    pub count: i32,
    pub components: DataComponentPatch,
}

impl ItemStack {
    pub fn new(item: i32, count: i32) -> Self {
        Self {
            item,
            count,
            components: DataComponentPatch::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count <= 0
    }

    fn encode_with(&self, out: &mut Vec<u8>, delimited: bool) {
        if self.is_empty() {
            write_varint(out, 0);
            return;
        }
        write_varint(out, self.count);
        write_varint(out, self.item);
        self.components.encode_with(out, delimited);
    }

    fn decode_with(input: &mut &[u8], delimited: bool) -> Result<Self> {
        let count = read_varint(input)?;
        if count <= 0 {
            return Ok(Self::default());
        }
        Ok(Self {
            count,
            item: read_varint(input)?,
            components: DataComponentPatch::decode_with(input, delimited)?,
        })
    }
}

impl Encode for ItemStack {
    fn encode(&self, out: &mut Vec<u8>) {
        self.encode_with(out, false);
    }
}

impl Decode<'_> for ItemStack {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Self::decode_with(input, false)
    }
}

/// Same bytes as [`ItemStack`], but an empty stack is an error.
#[derive(Debug, Clone, PartialEq)]
pub struct NonEmptyItemStack(pub ItemStack);

impl Encode for NonEmptyItemStack {
    fn encode(&self, out: &mut Vec<u8>) {
        self.0.encode(out);
    }
}

impl Decode<'_> for NonEmptyItemStack {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let stack = ItemStack::decode(input)?;
        if stack.is_empty() {
            return Err(DecodeError::Other("empty item stack not allowed".into()));
        }
        Ok(Self(stack))
    }
}

/// Client-originated stack (creative slot): every component payload is length-prefixed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UntrustedItemStack(pub ItemStack);

impl Encode for UntrustedItemStack {
    fn encode(&self, out: &mut Vec<u8>) {
        self.0.encode_with(out, true);
    }
}

impl Decode<'_> for UntrustedItemStack {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        ItemStack::decode_with(input, true).map(Self)
    }
}

/// A stack that cannot be empty, nested inside components.
#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct ItemStackTemplate {
    #[mcr(varint)]
    pub item: i32,
    #[mcr(varint)]
    pub count: i32,
    pub components: DataComponentPatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct HashedComponent {
    #[mcr(varint)]
    pub type_id: i32,
    pub hash: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct HashedPatchMap {
    pub added: LengthPrefixed<HashedComponent, 256>,
    pub removed: LengthPrefixed<VarInt, 256>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HashedItem {
    #[mcr(varint)]
    pub item: i32,
    #[mcr(varint)]
    pub count: i32,
    pub components: HashedPatchMap,
}

/// Stack as the server compares it after a container click: components appear only as hashes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HashedStack {
    #[default]
    Empty,
    Item(HashedItem),
}

impl Encode for HashedStack {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Empty => false.encode(out),
            Self::Item(item) => {
                true.encode(out);
                item.encode(out);
            }
        }
    }
}

impl Decode<'_> for HashedStack {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        if bool::decode(input)? {
            HashedItem::decode(input).map(Self::Item)
        } else {
            Ok(Self::Empty)
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;
    use crate::test_util::roundtrip;
    use crate::types::component::{DataComponent, Unit};

    fn damaged(count: i32, damage: i32) -> ItemStack {
        ItemStack {
            item: 5,
            count,
            components: DataComponentPatch {
                added: vec![DataComponent::Damage(VarInt(damage))],
                removed: vec![],
            },
        }
    }

    #[test]
    fn plain_stack_bytes() {
        assert_eq!(roundtrip(&ItemStack::new(1, 64)), [0x40, 0x01, 0, 0]);
    }

    #[test]
    fn empty_stack() {
        assert_eq!(roundtrip(&ItemStack::default()), [0]);
        assert_eq!(
            from_bytes_exact::<ItemStack>(&[0xff, 0xff, 0xff, 0xff, 0x0f]).unwrap(),
            ItemStack::default()
        );
        assert!(from_bytes_exact::<NonEmptyItemStack>(&[0]).is_err());
        assert_eq!(
            roundtrip(&NonEmptyItemStack(ItemStack::new(2, 1))),
            [1, 2, 0, 0]
        );
    }

    #[test]
    fn stack_with_components_bytes() {
        let mut stack = damaged(1, 5);
        stack
            .components
            .added
            .push(DataComponent::Unbreakable(Unit));
        stack.components.removed.push(20);
        assert_eq!(roundtrip(&stack), [1, 5, 2, 1, 3, 5, 4, 20]);
    }

    #[test]
    fn untrusted_stack_is_length_prefixed() {
        let stack = UntrustedItemStack(damaged(1, 5));
        let bytes = to_bytes(&stack);
        assert_eq!(bytes, [1, 5, 1, 0, 3, 1, 5]);
        assert_eq!(
            from_bytes_exact::<UntrustedItemStack>(&bytes).unwrap(),
            stack
        );
        assert_eq!(roundtrip(&UntrustedItemStack::default()), [0]);
        assert!(from_bytes_exact::<UntrustedItemStack>(&[1, 5, 1, 0, 3, 9, 5]).is_err());
    }

    #[test]
    fn template_bytes() {
        let t = ItemStackTemplate {
            item: 300,
            count: 3,
            components: DataComponentPatch::default(),
        };
        assert_eq!(roundtrip(&t), [0xac, 0x02, 3, 0, 0]);
    }

    #[test]
    fn named_sword_bytes() {
        use crate::types::Component;
        let stack = ItemStack {
            item: 5,
            count: 1,
            components: DataComponentPatch {
                added: vec![DataComponent::CustomName(Component::default())],
                removed: vec![],
            },
        };
        let bytes = roundtrip(&stack);
        assert_eq!(&bytes[..5], [1, 5, 1, 0, 6]);
    }

    #[test]
    fn hashed_stack_bytes() {
        assert_eq!(roundtrip(&HashedStack::Empty), [0]);
        let stack = HashedStack::Item(HashedItem {
            item: 1,
            count: 2,
            components: HashedPatchMap {
                added: LengthPrefixed(vec![
                    HashedComponent {
                        type_id: 3,
                        hash: 0x0102_0304,
                    },
                    HashedComponent {
                        type_id: 4,
                        hash: -1,
                    },
                ]),
                removed: LengthPrefixed(vec![VarInt(9)]),
            },
        });
        assert_eq!(
            roundtrip(&stack),
            [1, 1, 2, 2, 3, 1, 2, 3, 4, 4, 255, 255, 255, 255, 1, 9]
        );
    }

    #[test]
    fn hashed_stack_limits() {
        let mut bytes = vec![1, 1, 1];
        write_varint(&mut bytes, 257);
        bytes.extend(std::iter::repeat_n(0, 257 * 5 + 1));
        assert!(HashedStack::decode(&mut &bytes[..]).is_err());
        let mut bytes = vec![1, 1, 1, 0];
        write_varint(&mut bytes, 257);
        bytes.extend(std::iter::repeat_n(0, 300));
        assert!(HashedStack::decode(&mut &bytes[..]).is_err());
    }

    #[test]
    fn hostile_stacks_fail_cleanly() {
        assert!(ItemStack::decode(&mut &[1, 1, 0xff, 0xff, 0xff, 0xff, 0x07, 0][..]).is_err());
        assert!(ItemStack::decode(&mut &[1][..]).is_err());
        assert!(ItemStack::decode(&mut &[1, 1, 1, 0, 0xff, 0xff, 0xff, 0xff, 0x0f][..]).is_err());
        assert!(HashedStack::decode(&mut &[1, 1, 1, 0xff, 0xff, 0xff, 0xff, 0x07][..]).is_err());
    }
}
