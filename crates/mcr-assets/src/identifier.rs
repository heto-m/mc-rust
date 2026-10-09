use std::fmt;

pub const DEFAULT_NAMESPACE: &str = "minecraft";

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Identifier {
    namespace: Box<str>,
    path: Box<str>,
}

impl Identifier {
    pub fn new(namespace: &str, path: &str) -> Self {
        Self {
            namespace: namespace.into(),
            path: path.into(),
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.split_once(':') {
            Some((ns, path)) => Self::new(ns, path),
            None => Self::new(DEFAULT_NAMESPACE, s),
        }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn resource_path(&self, kind: &str, extension: &str) -> String {
        format!("assets/{}/{kind}/{}{extension}", self.namespace, self.path)
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_namespace() {
        let id = Identifier::parse("block/stone");
        assert_eq!(id.to_string(), "minecraft:block/stone");
        assert_eq!(
            id.resource_path("models", ".json"),
            "assets/minecraft/models/block/stone.json"
        );
    }

    #[test]
    fn parses_explicit_namespace() {
        let id = Identifier::parse("mod:thing");
        assert_eq!((id.namespace(), id.path()), ("mod", "thing"));
    }
}
