use mcr_buf::{Decode, Encode, Result};

#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Component {}

impl Encode for Component {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("text components")
    }
}

impl Decode<'_> for Component {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("text components")
    }
}
