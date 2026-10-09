use mcr_buf::{Decode, Encode, Result};

#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct SoundEvent {}

impl Encode for SoundEvent {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("sound events")
    }
}

impl Decode<'_> for SoundEvent {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("sound events")
    }
}
