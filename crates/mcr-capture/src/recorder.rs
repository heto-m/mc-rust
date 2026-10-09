use anyhow::Result;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const DEFAULT_CAP: usize = 20;
const UNCAPPED: &[&str] = &["registry_data"];
const LARGE_CAP: usize = 60;
const LARGE: &[&str] = &[
    "level_chunk_with_light",
    "commands",
    "update_tags",
    "update_recipes",
    "login",
    "player_info_update",
    "set_entity_data",
    "add_entity",
    "update_attributes",
    "section_blocks_update",
    "update_advancements",
    "recipe_book_add",
    "set_equipment",
    "container_set_content",
    "custom_payload",
];

pub fn cap_for(name: &str) -> usize {
    if UNCAPPED.contains(&name) {
        usize::MAX
    } else if LARGE.contains(&name) {
        LARGE_CAP
    } else {
        DEFAULT_CAP
    }
}

#[derive(Debug)]
pub struct Recorder {
    root: PathBuf,
    index: BufWriter<File>,
    counts: HashMap<(String, &'static str, String), usize>,
    seq: u64,
    start: Instant,
}

impl Recorder {
    pub fn create(root: &Path) -> Result<Self> {
        if root.exists() {
            fs::remove_dir_all(root)?;
        }
        fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            index: BufWriter::new(File::create(root.join("index.jsonl"))?),
            counts: HashMap::new(),
            seq: 0,
            start: Instant::now(),
        })
    }

    pub fn record(
        &mut self,
        state: &str,
        direction: &'static str,
        id: i32,
        name: &str,
        payload: &[u8],
    ) -> Result<()> {
        let count = self
            .counts
            .entry((state.to_owned(), direction, name.to_owned()))
            .or_insert(0);
        if *count >= cap_for(name) {
            return Ok(());
        }
        *count += 1;
        let seq = self.seq;
        self.seq += 1;
        let dir = self
            .root
            .join(state)
            .join(direction)
            .join(name.replace('/', "_"));
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{seq}.bin")), payload)?;
        let line = serde_json::json!({
            "seq": seq,
            "state": state,
            "direction": direction,
            "id": id,
            "name": name,
            "len": payload.len(),
            "timestamp_ms": self.start.elapsed().as_millis() as u64,
        });
        writeln!(self.index, "{line}")?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<HashMap<(String, &'static str, String), usize>> {
        self.index.flush()?;
        Ok(self.counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps() {
        assert_eq!(cap_for("registry_data"), usize::MAX);
        assert_eq!(cap_for("level_chunk_with_light"), LARGE_CAP);
        assert_eq!(cap_for("move_entity_pos"), DEFAULT_CAP);
    }

    #[test]
    fn records_up_to_cap() {
        let dir = std::env::temp_dir().join(format!("mcr-capture-test-{}", std::process::id()));
        let mut r = Recorder::create(&dir).unwrap();
        for _ in 0..DEFAULT_CAP + 5 {
            r.record("play", "clientbound", 1, "foo", &[1, 2, 3])
                .unwrap();
        }
        let counts = r.finish().unwrap();
        assert_eq!(
            counts[&("play".into(), "clientbound", "foo".into())],
            DEFAULT_CAP
        );
        let index = fs::read_to_string(dir.join("index.jsonl")).unwrap();
        assert_eq!(index.lines().count(), DEFAULT_CAP);
        assert_eq!(
            fs::read(dir.join("play/clientbound/foo/0.bin")).unwrap(),
            [1, 2, 3]
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
