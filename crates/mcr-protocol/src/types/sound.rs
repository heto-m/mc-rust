use mcr_buf::{Decode, Encode, Identifier};

/// Inline form of a sound holder; registered sounds travel as `IdOr::Id`.
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SoundEvent {
    pub location: Identifier,
    pub fixed_range: Option<f32>,
}

#[cfg(test)]
mod tests {
    use mcr_buf::IdOr;

    use super::*;
    use crate::test_util::roundtrip;

    fn sound(range: Option<f32>) -> SoundEvent {
        SoundEvent {
            location: Identifier::parse("a:b").unwrap(),
            fixed_range: range,
        }
    }

    #[test]
    fn variable_range() {
        let bytes = roundtrip(&sound(None));
        assert_eq!(bytes, [3, b'a', b':', b'b', 0]);
    }

    #[test]
    fn fixed_range() {
        let bytes = roundtrip(&sound(Some(16.0)));
        assert_eq!(bytes, [3, b'a', b':', b'b', 1, 0x41, 0x80, 0, 0]);
    }

    #[test]
    fn inside_holder() {
        let inline = IdOr::Inline(sound(None));
        assert_eq!(roundtrip(&inline), [0, 3, b'a', b':', b'b', 0]);
        assert_eq!(roundtrip(&IdOr::<SoundEvent>::Id(7)), [8]);
    }
}
