use mcr_buf::{Decode, Encode, Result};

#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct ItemStack {}

impl Encode for ItemStack {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("item stacks")
    }
}

impl Decode<'_> for ItemStack {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("item stacks")
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct HashedStack {}

impl Encode for HashedStack {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("hashed stacks")
    }
}

impl Decode<'_> for HashedStack {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("hashed stacks")
    }
}
