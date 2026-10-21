//! The player's own resource packs: `options.txt` `resourcePacks`, the `resourcepacks` folder, the
//! built-in packs from the asset index, and `PackSelectionScreen`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use mcr_assets::{AssetSource, PackMeta, ZipSource};
use mcr_settings::Options;
use mcr_ui::{
    Button, ConfirmScreen, Env, Key, MouseButton, RenderCx, Screen, ScreenAction, ScrollList,
    SpriteId, SpriteReleaser, UiInput, WidgetEventKind, WidgetSet, contains, draw_menu_background,
    tile_texture,
};

use crate::options_list::{FOOTER, HEADER};
use crate::settings::SharedSettings;

pub const HIGH_CONTRAST: &str = "high_contrast";
const BUILTIN: [&str; 2] = ["programmer_art", HIGH_CONTRAST];
const MAX_PACKS: usize = 256;
const MAX_DESCRIPTION: usize = 256;
const MAX_ICON_BYTES: usize = 4 << 20;
const MAX_ICON_SIDE: u32 = 512;
const MAX_COPY_ENTRIES: usize = 20_000;
const MAX_COPY_DEPTH: usize = 16;
const ROW: i32 = 36;
const LIST_WIDTH: i32 = 200;
const OPEN_FOLDER: u32 = 1;
const DONE: u32 = 2;
const WHITE: u32 = 0xFFFF_FFFF;
const GRAY: u32 = 0xFF80_8080;

/// Where the player's packs come from.
#[derive(Debug, Clone, Default)]
pub struct PackHost {
    pub dir: PathBuf,
    /// Built-in pack ids with the zip the asset index stores them in.
    pub builtin: Vec<(String, PathBuf)>,
}

/// A pack's `pack.png`, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct PackIcon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackInfo {
    /// The `options.txt` id: `file/<name>` or a built-in id.
    pub id: String,
    pub title: String,
    pub description: String,
    pub path: PathBuf,
    /// `None` shows `unknown_pack`.
    pub icon: Option<Rc<PackIcon>>,
}

fn plain(value: &serde_json::Value, depth: usize, out: &mut String) {
    if depth > 8 || out.len() > MAX_DESCRIPTION {
        return;
    }
    match value {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Array(items) => {
            for v in items.iter().take(64) {
                plain(v, depth + 1, out);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(t) = map.get("text").or_else(|| map.get("translate")) {
                plain(t, depth + 1, out);
            }
            if let Some(extra) = map.get("extra") {
                plain(extra, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// The description as plain text, whether `pack.mcmeta` gives a string or a text component.
fn description(meta: &PackMeta) -> String {
    let raw = &meta.description;
    let mut out = String::new();
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(v) if !v.is_string() => plain(&v, 0, &mut out),
        _ => out.push_str(raw),
    }
    out.chars().take(MAX_DESCRIPTION).collect()
}

fn read_meta(path: &Path) -> Option<PackMeta> {
    let bytes = if path.is_dir() {
        std::fs::read(path.join("pack.mcmeta")).ok()?
    } else {
        ZipSource::open(path).ok()?.read("pack.mcmeta").ok()??
    };
    PackMeta::parse(&bytes).ok()
}

fn read_root_file(path: &Path, name: &str) -> Option<Vec<u8>> {
    if path.is_dir() {
        let file = std::fs::File::open(path.join(name)).ok()?;
        let meta = file.metadata().ok()?;
        if !meta.is_file() || meta.len() > MAX_ICON_BYTES as u64 {
            return None;
        }
        std::fs::read(path.join(name)).ok()
    } else {
        ZipSource::open(path)
            .ok()?
            .read(name)
            .ok()?
            .filter(|b| b.len() <= MAX_ICON_BYTES)
    }
}

fn read_icon(path: &Path) -> Option<Rc<PackIcon>> {
    let bytes = read_root_file(path, "pack.png")?;
    let (width, height, rgba) = crate::status::decode_png_rgba(&bytes, |w, h| {
        (1..=MAX_ICON_SIDE).contains(&w) && (1..=MAX_ICON_SIDE).contains(&h)
    })?;
    Some(Rc::new(PackIcon {
        width,
        height,
        rgba,
    }))
}

/// `minecraft/resourcepacks/<id>.zip` in the asset index, resolved to its object file.
fn builtin_path(assets_dir: &Path, index: &str, id: &str) -> Option<PathBuf> {
    let text =
        std::fs::read_to_string(assets_dir.join("indexes").join(format!("{index}.json"))).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let hash = json
        .get("objects")?
        .get(format!("minecraft/resourcepacks/{id}.zip"))?
        .get("hash")?
        .as_str()?;
    if hash.len() < 2 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let path = assets_dir.join("objects").join(&hash[..2]).join(hash);
    path.is_file().then_some(path)
}

impl PackHost {
    pub fn new(game_dir: &Path, assets_dir: &Path, index: Option<&str>) -> Self {
        let builtin = index
            .map(|index| {
                BUILTIN
                    .iter()
                    .filter_map(|id| Some(((*id).to_owned(), builtin_path(assets_dir, index, id)?)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            dir: game_dir.join("resourcepacks"),
            builtin,
        }
    }

    pub fn has_builtin(&self, id: &str) -> bool {
        self.builtin.iter().any(|(b, _)| b == id)
    }

    fn file_path(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return None;
        }
        let path = self.dir.join(name);
        path.exists().then_some(path)
    }

    /// Selected packs that exist, lowest priority first, as the resource stack takes them.
    pub fn resolve(&self, selected: &[String]) -> Vec<PathBuf> {
        selected
            .iter()
            .take(MAX_PACKS)
            .filter_map(|id| match id.strip_prefix("file/") {
                Some(name) => self.file_path(name),
                None => self
                    .builtin
                    .iter()
                    .find(|(b, _)| b == id)
                    .map(|(_, p)| p.clone()),
            })
            .collect()
    }

    /// Every pack the player can pick: built-ins first, then the folder's packs by name.
    pub fn available(&self) -> Vec<PackInfo> {
        let mut out: Vec<PackInfo> = self
            .builtin
            .iter()
            .map(|(id, path)| PackInfo {
                id: id.clone(),
                title: format!("lang:resourcePack.{id}.name"),
                description: read_meta(path).map(|m| description(&m)).unwrap_or_default(),
                path: path.clone(),
                icon: read_icon(path),
            })
            .collect();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .map(|d| d.flatten().map(|e| e.path()).take(MAX_PACKS).collect())
            .unwrap_or_default();
        files.sort();
        for path in files {
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            let is_zip = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
            if !path.is_dir() && !is_zip {
                continue;
            }
            let Some(meta) = read_meta(&path) else {
                continue;
            };
            out.push(PackInfo {
                id: format!("file/{name}"),
                title: name,
                description: description(&meta),
                icon: read_icon(&path),
                path,
            });
        }
        out
    }
}

/// `PackRepository#addPack`/`removePack` for a built-in id, as the high contrast toggle does.
pub fn select_builtin(options: &mut Options, id: &str, on: bool) {
    let list = &mut options.resource_packs.0;
    list.retain(|p| p != id);
    if on {
        list.push(id.to_owned());
    }
}

/// `PackSelectionScreen`: available packs on the left, selected ones on the right, top first.
pub struct PackScreen {
    settings: SharedSettings,
    host: PackHost,
    available: Vec<PackInfo>,
    /// Highest priority first, like the screen shows them.
    selected: Vec<PackInfo>,
    lists: [ScrollList; 2],
    widgets: WidgetSet,
    gui: (i32, i32),
    icons: RefCell<HashMap<String, Option<SpriteId>>>,
    releaser: RefCell<Option<SpriteReleaser>>,
    /// Bumped by the drop confirmation when it copied packs in.
    copied: Rc<Cell<u32>>,
    seen_copied: u32,
}

impl Drop for PackScreen {
    fn drop(&mut self) {
        if let Some(releaser) = self.releaser.get_mut() {
            releaser.release(self.icons.get_mut().drain().filter_map(|(_, id)| id));
        }
    }
}

/// A pack `PackDetector` would take: a folder or a zip with `pack.mcmeta` at its root.
pub fn is_pack(path: &Path) -> bool {
    read_meta(path).is_some()
}

/// Copies `src` into `dir` under its own name without following symlinks or replacing anything.
fn copy_into(src: &Path, dir: &Path) -> std::io::Result<()> {
    let name = src
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?;
    let mut budget = MAX_COPY_ENTRIES;
    copy_tree(src, &dir.join(name), 0, &mut budget)
}

fn copy_tree(src: &Path, dest: &Path, depth: usize, budget: &mut usize) -> std::io::Result<()> {
    *budget = budget
        .checked_sub(1)
        .ok_or_else(|| std::io::Error::other("too many files"))?;
    let kind = std::fs::symlink_metadata(src)?.file_type();
    if kind.is_symlink() {
        return Err(std::io::Error::other("symbolic link"));
    }
    if kind.is_dir() {
        if depth >= MAX_COPY_DEPTH {
            return Err(std::io::Error::other("too deep"));
        }
        std::fs::create_dir_all(dest)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(
                &entry.path(),
                &dest.join(entry.file_name()),
                depth + 1,
                budget,
            )?;
        }
        return Ok(());
    }
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)?;
    std::io::copy(&mut std::fs::File::open(src)?, &mut out)?;
    Ok(())
}

impl std::fmt::Debug for PackScreen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackScreen").finish_non_exhaustive()
    }
}

impl PackScreen {
    pub fn new(settings: SharedSettings, host: PackHost) -> Self {
        let all = host.available();
        let chosen = settings.borrow().file.options.resource_packs.0.clone();
        let selected: Vec<PackInfo> = chosen
            .iter()
            .rev()
            .filter_map(|id| all.iter().find(|p| p.id == *id).cloned())
            .collect();
        let available = all
            .into_iter()
            .filter(|p| !selected.iter().any(|s| s.id == p.id))
            .collect();
        Self {
            settings,
            host,
            available,
            selected,
            lists: [
                ScrollList::new([0, 0, 0, 0], ROW),
                ScrollList::new([0, 0, 0, 0], ROW),
            ],
            widgets: WidgetSet::default(),
            gui: (0, 0),
            icons: RefCell::default(),
            releaser: RefCell::default(),
            copied: Rc::new(Cell::new(0)),
            seen_copied: 0,
        }
    }

    fn reload(&mut self) {
        let all = self.host.available();
        let known: Vec<String> = self.selected.iter().map(|p| p.id.clone()).collect();
        self.selected = known
            .iter()
            .filter_map(|id| all.iter().find(|p| p.id == *id).cloned())
            .collect();
        self.available = all.into_iter().filter(|p| !known.contains(&p.id)).collect();
        self.lists[0].set_count(self.available.len());
        self.lists[1].set_count(self.selected.len());
    }

    fn icon(&self, cx: &mut RenderCx<'_, '_>, pack: &PackInfo) -> Option<SpriteId> {
        let icon = pack.icon.as_ref()?;
        self.releaser
            .borrow_mut()
            .get_or_insert_with(|| cx.frame.ui().sprites_mut().releaser());
        let mut cache = self.icons.borrow_mut();
        if !cache.contains_key(&pack.id) && cache.len() >= MAX_PACKS {
            for (_, id) in cache.drain() {
                if let Some(id) = id {
                    cx.frame.ui().sprites_mut().free_raw(id);
                }
            }
        }
        *cache.entry(pack.id.clone()).or_insert_with(|| {
            cx.frame
                .ui()
                .sprites_mut()
                .add_raw(icon.width, icon.height, &icon.rgba)
        })
    }

    /// `PackSelectionScreen#onFilesDrop`: asks, then copies the packs into the folder.
    fn drop_confirmation(&self, files: &[PathBuf]) -> ConfirmScreen {
        let (packs, rejected): (Vec<PathBuf>, Vec<PathBuf>) =
            files.iter().cloned().partition(|p| is_pack(p));
        let names = |paths: &[PathBuf]| {
            paths
                .iter()
                .filter_map(|p| p.file_name()?.to_str().map(str::to_owned))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let (dir, copied) = (self.host.dir.clone(), self.copied.clone());
        let rejected_names = names(&rejected);
        let yes = move || {
            let _ = std::fs::create_dir_all(&dir);
            for pack in &packs {
                if let Err(e) = copy_into(pack, &dir) {
                    eprintln!("packs: could not copy {}: {e}", pack.display());
                }
            }
            if !packs.is_empty() {
                copied.set(copied.get().wrapping_add(1));
            }
            let mut actions = vec![ScreenAction::Pop];
            if !rejected_names.is_empty() {
                actions.push(ScreenAction::Push(Box::new(ConfirmScreen::new(
                    "lang:pack.dropRejected.title",
                    &format!("lang:pack.dropRejected.message|{rejected_names}"),
                    vec![("lang:gui.ok".into(), Box::new(|| vec![ScreenAction::Pop]))],
                ))));
            }
            actions
        };
        ConfirmScreen::new(
            "lang:pack.dropConfirm",
            &names(files),
            vec![
                ("lang:gui.yes".into(), Box::new(yes)),
                ("lang:gui.no".into(), Box::new(|| vec![ScreenAction::Pop])),
            ],
        )
    }

    fn list_x(&self, side: usize) -> i32 {
        if side == 0 {
            self.gui.0 / 2 - 4 - LIST_WIDTH
        } else {
            self.gui.0 / 2 + 4
        }
    }

    /// Writes the selection back, keeping ids this screen does not list (such as `vanilla`).
    fn apply(&self) {
        let shown: Vec<&str> = self
            .available
            .iter()
            .chain(&self.selected)
            .map(|p| p.id.as_str())
            .collect();
        let mut ids: Vec<String> = self
            .settings
            .borrow()
            .file
            .options
            .resource_packs
            .0
            .iter()
            .filter(|id| !shown.contains(&id.as_str()))
            .cloned()
            .collect();
        ids.extend(self.selected.iter().rev().map(|p| p.id.clone()));
        let high_contrast = ids.iter().any(|i| i == HIGH_CONTRAST);
        let mut s = self.settings.borrow_mut();
        if s.file.options.resource_packs.0 != ids {
            s.edit(|f| {
                f.options.resource_packs.0 = ids;
                f.options.high_contrast = high_contrast;
            });
        }
    }

    fn toggle(&mut self, side: usize, index: usize) {
        if side == 0 {
            if index < self.available.len() {
                let pack = self.available.remove(index);
                self.selected.insert(0, pack);
            }
        } else if index < self.selected.len() {
            let pack = self.selected.remove(index);
            self.available.push(pack);
        }
        self.lists[0].set_count(self.available.len());
        self.lists[1].set_count(self.selected.len());
    }

    fn draw_list(&self, cx: &mut RenderCx<'_, '_>, side: usize) {
        let packs = if side == 0 {
            &self.available
        } else {
            &self.selected
        };
        let list = &self.lists[side];
        let x = self.list_x(side);
        let title = cx.lang(if side == 0 {
            "pack.available.title"
        } else {
            "pack.selected.title"
        });
        let [_, ly, _, lh] = list.rect();
        cx.centered_text(
            &format!("§n§l{title}"),
            x + LIST_WIDTH / 2,
            ly - 12,
            WHITE,
            true,
        );
        cx.frame.push_scissor(x, ly, x + LIST_WIDTH, ly + lh);
        let (mx, my) = cx.mouse;
        for index in list.visible_range() {
            let Some(pack) = packs.get(index) else {
                continue;
            };
            let y = list.item_top(index);
            if contains([x, y, LIST_WIDTH, ROW], mx, my) && contains(list.rect(), mx, my) {
                cx.frame
                    .fill_rect(x, y, x + LIST_WIDTH, y + ROW - 4, 0x40FF_FFFF);
            }
            if let Some(icon) = self.icon(cx, pack) {
                cx.frame.sprite(icon, x + 2, y, 32, 32);
            } else if let Some((iw, ih)) = cx.assets.size("misc/unknown_pack") {
                cx.texture_scaled(
                    "misc/unknown_pack",
                    (0, 0, iw as i32, ih as i32),
                    (x + 2, y, 32, 32),
                );
            }
            let title = match pack.title.strip_prefix("lang:") {
                Some(key) => cx.lang(key),
                None => pack.title.clone(),
            };
            cx.text(&title, x + 38, y + 1, WHITE, true);
            let mut line = String::new();
            let mut lines = Vec::new();
            for word in pack.description.split_whitespace() {
                let next = if line.is_empty() {
                    word.to_owned()
                } else {
                    format!("{line} {word}")
                };
                if !line.is_empty() && cx.text_width(&next) > LIST_WIDTH - 44 {
                    lines.push(std::mem::replace(&mut line, word.to_owned()));
                } else {
                    line = next;
                }
            }
            lines.push(line);
            for (i, l) in lines.iter().take(2).enumerate() {
                cx.text(l, x + 38, y + 12 + i as i32 * 10, GRAY, true);
            }
        }
        cx.frame.pop_scissor();
        list.render_scrollbar(cx, x + LIST_WIDTH + 2);
    }
}

impl Screen for PackScreen {
    fn init(&mut self, width: i32, height: i32) {
        self.gui = (width, height);
        let top = HEADER + 20;
        let h = (height - top - FOOTER - 4).max(0);
        for side in 0..2 {
            let x = self.list_x(side);
            self.lists[side].set_rect([x, top, LIST_WIDTH, h]);
        }
        self.lists[0].set_count(self.available.len());
        self.lists[1].set_count(self.selected.len());
        self.widgets.clear();
        self.widgets.add(Button::new(
            OPEN_FOLDER,
            [width / 2 - 154, height - FOOTER + 6, 150, 20],
            "lang:pack.openFolder",
        ));
        self.widgets.add(Button::new(
            DONE,
            [width / 2 + 4, height - FOOTER + 6, 150, 20],
            "lang:gui.done",
        ));
    }

    fn render(&mut self, cx: &mut RenderCx<'_, '_>) {
        let (w, h) = self.gui;
        draw_menu_background(cx);
        tile_texture(
            cx,
            "menu_list_background",
            [0, HEADER, w, (h - HEADER - FOOTER).max(0)],
        );
        let title = cx.lang("resourcePack.title");
        cx.centered_text(&title, w / 2, 12, WHITE, true);
        self.draw_list(cx, 0);
        self.draw_list(cx, 1);
        self.widgets.render(cx);
    }

    fn tick(&mut self, _env: &mut Env<'_>, _out: &mut Vec<ScreenAction>) {
        if self.seen_copied != self.copied.get() {
            self.seen_copied = self.copied.get();
            self.reload();
        }
    }

    fn files_dropped(&mut self, paths: &[PathBuf], out: &mut Vec<ScreenAction>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let paths = &paths[..paths.len().min(MAX_PACKS)];
        out.push(ScreenAction::Push(Box::new(self.drop_confirmation(paths))));
        true
    }

    fn input(&mut self, env: &mut Env<'_>, input: &UiInput, out: &mut Vec<ScreenAction>) -> bool {
        if let UiInput::Key {
            key: Key::Escape, ..
        } = input
        {
            self.apply();
            out.push(ScreenAction::Pop);
            return true;
        }
        for side in 0..2 {
            let bar = self.list_x(side) + LIST_WIDTH + 2;
            if self.lists[side].input(input, bar) {
                return true;
            }
        }
        if let UiInput::MousePress {
            x,
            y,
            button: MouseButton::Left,
            ..
        } = *input
        {
            for side in 0..2 {
                if let Some(i) = self.lists[side].item_at(x, y) {
                    self.toggle(side, i);
                    out.push(ScreenAction::ClickSound);
                    return true;
                }
            }
        }
        let mut events = Vec::new();
        let used = self.widgets.input(env, input, &mut events);
        for e in events.iter().filter(|e| e.kind == WidgetEventKind::Pressed) {
            out.push(ScreenAction::ClickSound);
            match e.id {
                OPEN_FOLDER => {
                    let _ = std::fs::create_dir_all(&self.host.dir);
                    out.push(ScreenAction::OpenUrl(format!(
                        "file://{}",
                        self.host.dir.display()
                    )));
                }
                DONE => {
                    self.apply();
                    out.push(ScreenAction::Pop);
                }
                _ => {}
            }
        }
        used || matches!(input, UiInput::Key { .. } | UiInput::Char(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mcr-packs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("resourcepacks")).unwrap();
        dir
    }

    #[test]
    fn folder_packs_are_listed_and_resolved_safely() {
        let dir = temp("folder");
        let pack = dir.join("resourcepacks/Faithful");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"pack_format":64,"description":{"text":"Crisp","extra":[" textures"]}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("resourcepacks/notes.txt"), "x").unwrap();
        let host = PackHost::new(&dir, &dir.join("assets"), None);
        let available = host.available();
        assert_eq!(available.len(), 1);
        assert_eq!(available[0].id, "file/Faithful");
        assert_eq!(available[0].description, "Crisp textures");
        let resolved = host.resolve(&[
            "vanilla".into(),
            "file/Faithful".into(),
            "file/../escape".into(),
            "file/missing".into(),
        ]);
        assert_eq!(resolved, vec![pack]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn high_contrast_moves_to_the_top_and_back_out() {
        let mut o = Options::default();
        o.resource_packs.0 = vec!["vanilla".into(), HIGH_CONTRAST.into(), "file/a".into()];
        select_builtin(&mut o, HIGH_CONTRAST, true);
        assert_eq!(o.resource_packs.0, vec!["vanilla", "file/a", HIGH_CONTRAST]);
        select_builtin(&mut o, HIGH_CONTRAST, false);
        assert_eq!(o.resource_packs.0, vec!["vanilla", "file/a"]);
    }

    #[test]
    fn builtins_resolve_through_the_asset_index() {
        let dir = temp("index");
        let assets = dir.join("assets");
        std::fs::create_dir_all(assets.join("indexes")).unwrap();
        std::fs::create_dir_all(assets.join("objects/ab")).unwrap();
        std::fs::write(
            assets.join("indexes/34.json"),
            r#"{"objects":{"minecraft/resourcepacks/high_contrast.zip":{"hash":"abcd","size":1}}}"#,
        )
        .unwrap();
        std::fs::write(assets.join("objects/ab/abcd"), b"zip").unwrap();
        let host = PackHost::new(&dir, &assets, Some("34"));
        assert!(host.has_builtin(HIGH_CONTRAST));
        assert!(!host.has_builtin("programmer_art"));
        assert_eq!(
            host.resolve(&[HIGH_CONTRAST.into()]),
            vec![assets.join("objects/ab/abcd")]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut enc = png::Encoder::new(&mut bytes, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&vec![200; (width * height * 4) as usize])
            .unwrap();
        drop(w);
        bytes
    }

    fn pack_dir(root: &Path, name: &str, icon: Option<Vec<u8>>) -> PathBuf {
        let pack = root.join(name);
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(
            pack.join("pack.mcmeta"),
            r#"{"pack":{"pack_format":64,"description":"d"}}"#,
        )
        .unwrap();
        if let Some(icon) = icon {
            std::fs::write(pack.join("pack.png"), icon).unwrap();
        }
        pack
    }

    #[test]
    fn packs_show_their_icon_or_fall_back_to_unknown_pack() {
        let dir = temp("icons");
        pack_dir(&dir.join("resourcepacks"), "With", Some(png(16, 16)));
        pack_dir(&dir.join("resourcepacks"), "Without", None);
        pack_dir(&dir.join("resourcepacks"), "Huge", Some(png(600, 8)));
        let host = PackHost::new(&dir, &dir.join("assets"), None);
        let by_name = |n: &str| host.available().into_iter().find(|p| p.title == n).unwrap();
        let icon = by_name("With").icon.unwrap();
        assert_eq!((icon.width, icon.height, icon.rgba.len()), (16, 16, 1024));
        assert!(by_name("Without").icon.is_none());
        assert!(by_name("Huge").icon.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dropped_packs_are_checked_and_copied_without_replacing() {
        let dir = temp("drop");
        let source = dir.join("incoming");
        let good = pack_dir(&source, "Good", None);
        std::fs::write(good.join("data.txt"), "x").unwrap();
        let bad = source.join("notes");
        std::fs::create_dir_all(&bad).unwrap();
        assert!(is_pack(&good));
        assert!(!is_pack(&bad));
        let target = dir.join("resourcepacks");
        copy_into(&good, &target).unwrap();
        assert!(target.join("Good/pack.mcmeta").is_file());
        assert!(target.join("Good/data.txt").is_file());
        assert!(copy_into(&good, &target).is_err(), "existing pack is kept");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_drop_on_the_pack_screen_asks_before_copying() {
        let dir = temp("screen");
        let good = pack_dir(&dir.join("incoming"), "Good", None);
        let settings = crate::settings::Settings::load(&dir);
        let mut screen = PackScreen::new(settings, PackHost::new(&dir, &dir.join("assets"), None));
        let mut out = Vec::new();
        assert!(screen.files_dropped(&[good], &mut out));
        assert!(matches!(out.as_slice(), [ScreenAction::Push(_)]));
        assert!(
            !dir.join("resourcepacks/Good").exists(),
            "nothing copied before Yes"
        );
        assert!(!screen.files_dropped(&[], &mut Vec::new()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
