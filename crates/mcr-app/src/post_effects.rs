use mcr_assets::ResourceManager;
use mcr_render::{MAX_POST_CHAINS, PostChain, PostOverrides};

/// Vanilla's spectated-entity post effect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpectatorEffect {
    chain: Option<PostChain>,
    active: bool,
}

impl SpectatorEffect {
    /// Picks the chain for the camera entity's type id, or clears it for `None` (the player, or
    /// any view outside first person).
    pub fn check(&mut self, camera_type: Option<&str>) {
        self.chain = match camera_type {
            Some("minecraft:creeper") => Some(PostChain::Creeper),
            Some("minecraft:spider" | "minecraft:cave_spider") => Some(PostChain::Spider),
            Some("minecraft:enderman") => Some(PostChain::Invert),
            _ => None,
        };
        self.active = self.chain.is_some();
    }

    pub fn toggle(&mut self) {
        self.active = !self.active;
    }

    pub fn chain(&self) -> Option<PostChain> {
        self.chain.filter(|_| self.active)
    }
}

/// The resource stack's `post_effect/<chain>.json` files, for the chains whose shape we can follow.
pub fn pack_overrides(resources: &ResourceManager) -> PostOverrides {
    let mut overrides = PostOverrides::default();
    for chain in PostChain::ALL {
        let path = format!("assets/minecraft/post_effect/{}.json", chain.name());
        let Some(json) = resources
            .read(&path)
            .ok()
            .flatten()
            .and_then(|b| String::from_utf8(b).ok())
        else {
            continue;
        };
        if !overrides.set(chain, &json) {
            tracing::warn!(
                "post effect {} from the resource packs has another shape or custom shaders; using the built-in chain",
                chain.name()
            );
        }
    }
    overrides
}

/// The chains to run this frame: the server's list, then the spectated chain.
pub fn requested(server: &[String], spectated: Option<PostChain>, out: &mut Vec<PostChain>) {
    out.clear();
    out.extend(server.iter().filter_map(|id| PostChain::from_id(id)));
    out.extend(spectated);
    out.truncate(MAX_POST_CHAINS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_directory_retunes_only_the_chains_that_match() {
        let root = std::env::temp_dir().join(format!("mcr-post-pack-{}", std::process::id()));
        let dir = root.join("assets/minecraft/post_effect");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            root.join("pack.mcmeta"),
            r#"{"pack":{"pack_format":75,"description":""}}"#,
        )
        .unwrap();
        let invert = r#"{"passes":[
            {"vertex_shader":"minecraft:core/screenquad","fragment_shader":"minecraft:post/invert",
             "inputs":[{"sampler_name":"In","target":"minecraft:main"}],"output":"swap",
             "uniforms":{"InvertConfig":[{"name":"InverseAmount","type":"float","value":0.3}]}},
            {"vertex_shader":"minecraft:core/screenquad","fragment_shader":"minecraft:post/blit",
             "inputs":[{"sampler_name":"In","target":"swap"}],"output":"minecraft:main",
             "uniforms":{"BlitConfig":[{"name":"ColorModulate","type":"vec4","value":[1,1,1,1]}]}}]}"#;
        std::fs::write(dir.join("invert.json"), invert).unwrap();
        std::fs::write(dir.join("creeper.json"), r#"{"passes":[]}"#).unwrap();
        let mut resources = ResourceManager::new();
        resources.push_path(&root).unwrap();
        let overrides = pack_overrides(&resources);
        let _ = std::fs::remove_dir_all(&root);
        assert!(overrides.follows(PostChain::Invert));
        assert!(!overrides.follows(PostChain::Creeper));
        assert!(!overrides.follows(PostChain::Spider));
    }

    #[test]
    fn spectated_mobs_pick_vanilla_chains() {
        let mut e = SpectatorEffect::default();
        for (ty, chain) in [
            ("minecraft:creeper", Some(PostChain::Creeper)),
            ("minecraft:cave_spider", Some(PostChain::Spider)),
            ("minecraft:enderman", Some(PostChain::Invert)),
            ("minecraft:zombie", None),
        ] {
            e.check(Some(ty));
            assert_eq!(e.chain(), chain, "{ty}");
        }
        e.check(Some("minecraft:spider"));
        e.toggle();
        assert_eq!(e.chain(), None);
        e.toggle();
        assert_eq!(e.chain(), Some(PostChain::Spider));
        e.check(None);
        assert_eq!(e.chain(), None);
    }

    #[test]
    fn server_list_comes_first_and_unknown_ids_drop() {
        let server = ["minecraft:entity_outline", "foo:bar", "minecraft:invert"].map(String::from);
        let mut out = Vec::new();
        requested(&server, Some(PostChain::Creeper), &mut out);
        assert_eq!(out, [PostChain::Invert, PostChain::Creeper]);
        let many = vec!["minecraft:blur".to_owned(); 16];
        requested(&many, Some(PostChain::Spider), &mut out);
        assert_eq!(out.len(), MAX_POST_CHAINS);
        assert!(!out.contains(&PostChain::Spider));
    }
}
