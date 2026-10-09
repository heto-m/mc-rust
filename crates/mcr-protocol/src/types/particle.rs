use mcr_buf::{Decode, Encode, Result};

#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct Particle {}

impl Encode for Particle {
    fn encode(&self, _out: &mut Vec<u8>) {
        todo!("particles")
    }
}

impl Decode<'_> for Particle {
    fn decode(_input: &mut &[u8]) -> Result<Self> {
        todo!("particles")
    }
}
