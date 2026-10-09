use mcr_buf::{Decode, Encode, Result};

#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct EntityDataValues {}

impl Encode for EntityDataValues {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("entity data")
    }
}

impl Decode<'_> for EntityDataValues {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("entity data")
    }
}
