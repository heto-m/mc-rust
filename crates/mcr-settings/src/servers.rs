use mcr_nbt::{Compound, Compression, Limits, List, Result, Value, read_disk, write_disk};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResourcePacks {
    Enabled,
    Disabled,
    #[default]
    Prompt,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerEntry {
    pub name: String,
    pub ip: String,
    /// Base64 of the server icon PNG, as stored in `servers.dat`.
    pub icon: Option<String>,
    pub resource_packs: ResourcePacks,
    pub accepted_code_of_conduct: i32,
    pub hidden: bool,
}

/// The multiplayer server list in `servers.dat`, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerList {
    pub servers: Vec<ServerEntry>,
}

impl ServerList {
    /// Reads gzip or uncompressed `servers.dat` bytes. A missing `servers` list is an empty list.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let (_, root) = read_disk(bytes, Limits::UNCOMPRESSED)?;
        let servers = root
            .get_list("servers")
            .map(List::as_compounds)
            .unwrap_or_default()
            .iter()
            .map(ServerEntry::from_nbt)
            .collect();
        Ok(Self { servers })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let list = List::Compound(self.servers.iter().map(ServerEntry::to_nbt).collect());
        let mut root = Compound::new();
        root.insert("servers", Value::List(list));
        write_disk("", &root, Compression::Gzip)
    }

    /// Entries that the server list screen shows; hidden entries stay on disk.
    pub fn visible(&self) -> impl Iterator<Item = &ServerEntry> {
        self.servers.iter().filter(|s| !s.hidden)
    }
}

impl ServerEntry {
    fn from_nbt(tag: &Compound) -> Self {
        let resource_packs = match tag.get_bool("acceptTextures") {
            Some(true) => ResourcePacks::Enabled,
            Some(false) => ResourcePacks::Disabled,
            None => ResourcePacks::Prompt,
        };
        Self {
            name: tag.get_str("name").unwrap_or_default().to_owned(),
            ip: tag.get_str("ip").unwrap_or_default().to_owned(),
            icon: tag.get_str("icon").map(str::to_owned),
            resource_packs,
            accepted_code_of_conduct: tag.get_i32("acceptedCodeOfConduct").unwrap_or(0),
            hidden: tag.get_bool("hidden").unwrap_or(false),
        }
    }

    fn to_nbt(&self) -> Compound {
        let mut tag = Compound::new();
        tag.insert("name", self.name.as_str());
        tag.insert("ip", self.ip.as_str());
        if let Some(icon) = &self.icon {
            tag.insert("icon", icon.as_str());
        }
        match self.resource_packs {
            ResourcePacks::Enabled => tag.insert("acceptTextures", Value::Byte(1)),
            ResourcePacks::Disabled => tag.insert("acceptTextures", Value::Byte(0)),
            ResourcePacks::Prompt => None,
        };
        if self.accepted_code_of_conduct != 0 {
            tag.insert("acceptedCodeOfConduct", self.accepted_code_of_conduct);
        }
        tag.insert("hidden", Value::Byte(i8::from(self.hidden)));
        tag
    }
}
