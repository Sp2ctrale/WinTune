//! Icon gallery: a read-only icon catalogue ([`IconLibrary`]) that can be
//! shared with worker threads, a texture cache that decodes thumbnails in the
//! background ([`IconTextureCache`]), and the `gallery.pak` builder.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconCategory {
    All,
    FilesOnly,
    FoldersOnly,
    LightVariants,
    Clones,
}

impl IconCategory {
    /// The category whose filter shows the icon `name` (see
    /// [`IconLibrary::filter_names`]). A name that is in several categories
    /// gets the narrowest one (Clones, then Light, then Folders, then
    /// Files); an empty name has none, so it gets `All`.
    pub fn of(name: &str) -> Self {
        let lower = name.trim().to_lowercase();
        if lower.is_empty() {
            Self::All
        } else if lower.ends_with(".clone") {
            Self::Clones
        } else if lower.contains("_light") {
            Self::LightVariants
        } else if lower.starts_with("folder-") || lower == "folder" {
            Self::FoldersOnly
        } else {
            Self::FilesOnly
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakEntry {
    pub name: String,
    pub offset: u64,
    pub length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PakIndex {
    pub entries: Vec<PakEntry>,
}

/// Read-only icon catalogue: a folder of `.ico` files plus an optional
/// `gallery.pak`. The pack is read into memory rather than memory-mapped, so
/// the file stays free to be rebuilt while the gallery is open.
#[derive(Default)]
pub struct IconLibrary {
    names: Vec<String>,
    file_paths: HashMap<String, PathBuf>,
    pak_data: Vec<u8>,
    pak_entries: HashMap<String, (u64, u64)>,
}

impl IconLibrary {
    pub fn load(source_folder: &str, pak_path: &str) -> Self {
        let mut lib = Self::default();
        if Path::new(pak_path).is_file() {
            let _ = lib.load_pak(pak_path);
        }

        if let Ok(entries) = fs::read_dir(source_folder) {
            for path in entries.flatten().map(|e| e.path()) {
                let is_ico = path
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("ico"));
                if !path.is_file() || !is_ico {
                    continue;
                }
                if let Some(stem) = path.file_stem() {
                    let name = stem.to_string_lossy().to_string();
                    if !lib.pak_entries.contains_key(&name) {
                        lib.names.push(name.clone());
                    }
                    lib.file_paths.insert(name, path);
                }
            }
        }

        lib.names.sort_by_key(|n| n.to_lowercase());
        lib.names.dedup();
        lib
    }

    fn load_pak(&mut self, pak_path: &str) -> Result<(), String> {
        let data = fs::read(pak_path).map_err(|e| e.to_string())?;
        if data.len() < 12 || &data[0..4] != b"ICOP" {
            return Err("Invalid gallery.pak format".to_string());
        }
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&data[4..12]);
        // A damaged header must never panic (the pack is read at startup).
        let index_end = usize::try_from(u64::from_le_bytes(len_bytes))
            .ok()
            .and_then(|len| len.checked_add(12))
            .filter(|end| *end <= data.len())
            .ok_or_else(|| "Corrupt gallery.pak index".to_string())?;
        let index: PakIndex =
            serde_json::from_slice(&data[12..index_end]).map_err(|e| e.to_string())?;
        for entry in index.entries {
            self.pak_entries
                .insert(entry.name.clone(), (entry.offset, entry.length));
            self.names.push(entry.name);
        }
        self.pak_data = data;
        Ok(())
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn get_icon_bytes(&self, name: &str) -> Option<Vec<u8>> {
        if let Some(&(offset, length)) = self.pak_entries.get(name) {
            let range = usize::try_from(offset).ok().and_then(|start| {
                let end = start.checked_add(usize::try_from(length).ok()?)?;
                (end <= self.pak_data.len()).then_some(start..end)
            });
            if let Some(range) = range {
                return Some(self.pak_data[range].to_vec());
            }
        }
        self.file_paths.get(name).and_then(|p| fs::read(p).ok())
    }

    pub fn export_icon_to(&self, name: &str, dest_folder: &str) -> Result<String, String> {
        fs::create_dir_all(dest_folder).map_err(|e| e.to_string())?;
        let dest_path = Path::new(dest_folder).join(format!("{}.ico", name));
        let bytes = self
            .get_icon_bytes(name)
            .ok_or_else(|| format!("Icon not found in gallery: {}", name))?;
        fs::write(&dest_path, bytes).map_err(|e| e.to_string())?;
        Ok(dest_path.to_string_lossy().to_string())
    }

    pub fn filter_names(&self, query: &str, category: IconCategory) -> Vec<String> {
        let q = query.trim().to_lowercase();
        self.names
            .iter()
            .filter(|name| {
                let lower = name.to_lowercase();
                let cat_match = match category {
                    IconCategory::All => true,
                    IconCategory::FilesOnly => !lower.starts_with("folder-") && lower != "folder",
                    IconCategory::FoldersOnly => lower.starts_with("folder-") || lower == "folder",
                    IconCategory::LightVariants => lower.contains("_light"),
                    IconCategory::Clones => lower.ends_with(".clone"),
                };
                cat_match && (q.is_empty() || lower.contains(&q))
            })
            .cloned()
            .collect()
    }
}

struct DecodeJob {
    ctx: Context,
    library: Arc<IconLibrary>,
    name: String,
    generation: u64,
}

#[derive(Default)]
struct CacheState {
    /// Bumped by [`IconTextureCache::clear`]; results of older jobs are dropped.
    generation: u64,
    ready: HashMap<String, TextureHandle>,
    failed: HashSet<String>,
    pending: HashSet<String>,
}

/// Thumbnail textures decoded on a small worker pool, so scrolling a large
/// gallery never blocks the UI thread. Workers stop when the cache is dropped.
pub struct IconTextureCache {
    state: Arc<Mutex<CacheState>>,
    tx: Sender<DecodeJob>,
}

impl Default for IconTextureCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IconTextureCache {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<DecodeJob>();
        let rx = Arc::new(Mutex::new(rx));
        let state = Arc::new(Mutex::new(CacheState::default()));
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2)
            .clamp(1, 4);
        for i in 0..workers {
            let rx = Arc::clone(&rx);
            let state = Arc::clone(&state);
            std::thread::Builder::new()
                .name(format!("icon-decoder-{}", i))
                .spawn(move || decode_worker(rx, state))
                .expect("cannot spawn icon decoder thread");
        }
        Self { state, tx }
    }

    /// Returns the texture if it is ready; otherwise queues it for decoding
    /// (once) and returns `None`. A repaint is requested when it is ready.
    pub fn get(
        &self,
        ctx: &Context,
        library: &Arc<IconLibrary>,
        name: &str,
    ) -> Option<TextureHandle> {
        let mut st = self.state.lock().ok()?;
        if let Some(tex) = st.ready.get(name) {
            return Some(tex.clone());
        }
        if st.failed.contains(name) || st.pending.contains(name) {
            return None;
        }
        let job = DecodeJob {
            ctx: ctx.clone(),
            library: Arc::clone(library),
            name: name.to_string(),
            generation: st.generation,
        };
        if self.tx.send(job).is_ok() {
            st.pending.insert(name.to_string());
        } else {
            st.failed.insert(name.to_string());
        }
        None
    }

    pub fn is_failed(&self, name: &str) -> bool {
        self.state
            .lock()
            .map(|s| s.failed.contains(name))
            .unwrap_or(false)
    }

    /// Forgets every texture (e.g. after the library was reloaded).
    pub fn clear(&self) {
        if let Ok(mut st) = self.state.lock() {
            st.generation += 1;
            st.ready.clear();
            st.failed.clear();
            st.pending.clear();
        }
    }
}

fn decode_worker(rx: Arc<Mutex<Receiver<DecodeJob>>>, state: Arc<Mutex<CacheState>>) {
    loop {
        // The lock is held only while waiting for the next job; a closed
        // channel (cache dropped) ends the worker.
        let job = match rx.lock() {
            Ok(guard) => match guard.recv() {
                Ok(job) => job,
                Err(_) => return,
            },
            Err(_) => return,
        };

        let texture = job
            .library
            .get_icon_bytes(&job.name)
            .and_then(|bytes| decode_ico_thumbnail(&bytes, 48))
            .map(|img| {
                job.ctx
                    .load_texture(format!("icon:{}", job.name), img, TextureOptions::LINEAR)
            });

        let Ok(mut st) = state.lock() else { return };
        if st.generation != job.generation {
            continue; // library reloaded meanwhile: stale result
        }
        st.pending.remove(&job.name);
        match texture {
            Some(tex) => {
                st.ready.insert(job.name, tex);
            }
            None => {
                st.failed.insert(job.name);
            }
        }
        drop(st);
        job.ctx.request_repaint();
    }
}

/// Library + texture cache, as used by the UI.
pub struct IconGallery {
    library: Arc<IconLibrary>,
    textures: IconTextureCache,
}

impl Default for IconGallery {
    fn default() -> Self {
        Self {
            library: Arc::new(IconLibrary::default()),
            textures: IconTextureCache::new(),
        }
    }
}

impl IconGallery {
    pub fn reload(&mut self, source_folder: &str, pak_path: &str) {
        self.library = Arc::new(IconLibrary::load(source_folder, pak_path));
        self.textures.clear();
    }

    /// Shared handle for worker threads (e.g. icon export during an apply).
    pub fn library(&self) -> Arc<IconLibrary> {
        Arc::clone(&self.library)
    }

    pub fn names(&self) -> &[String] {
        self.library.names()
    }

    pub fn filter_names(&self, query: &str, category: IconCategory) -> Vec<String> {
        self.library.filter_names(query, category)
    }

    /// Non-blocking: `None` until the thumbnail has been decoded.
    pub fn texture(&self, ctx: &Context, name: &str) -> Option<TextureHandle> {
        self.textures.get(ctx, &self.library, name)
    }

    /// True once decoding was attempted and failed (missing or invalid icon).
    pub fn is_failed(&self, name: &str) -> bool {
        self.textures.is_failed(name)
    }
}

fn decode_ico_thumbnail(bytes: &[u8], target_size: u32) -> Option<ColorImage> {
    if let Ok(icon_dir) = ico::IconDir::read(Cursor::new(bytes)) {
        let best_entry = icon_dir
            .entries()
            .iter()
            .filter(|e| e.width() >= target_size)
            .min_by_key(|e| e.width())
            .or_else(|| icon_dir.entries().iter().max_by_key(|e| e.width()))?;

        if let Ok(image) = best_entry.decode() {
            let width = image.width() as usize;
            let height = image.height() as usize;
            let rgba = image.rgba_data().to_vec();
            return Some(ColorImage::from_rgba_unmultiplied([width, height], &rgba));
        }
    }

    let dyn_img = image::load_from_memory(bytes).ok()?;
    let resized = dyn_img.thumbnail(target_size, target_size).to_rgba8();
    let (w, h) = resized.dimensions();
    Some(ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        resized.as_raw(),
    ))
}

pub fn compress_ico_bytes(raw_bytes: &[u8]) -> Vec<u8> {
    let icon_dir = match ico::IconDir::read(Cursor::new(raw_bytes)) {
        Ok(dir) => dir,
        Err(_) => return raw_bytes.to_vec(),
    };
    let mut new_dir = ico::IconDir::new(ico::ResourceType::Icon);
    let mut kept_sizes = HashSet::new();

    for entry in icon_dir.entries() {
        let w = entry.width();
        let h = entry.height();
        if matches!(w, 16 | 32 | 48 | 64 | 128 | 256) && !kept_sizes.contains(&w) {
            if let Ok(decoded) = entry.decode() {
                let icon_image = ico::IconImage::from_rgba_data(w, h, decoded.rgba_data().to_vec());
                if let Ok(new_entry) = ico::IconDirEntry::encode_as_png(&icon_image) {
                    new_dir.add_entry(new_entry);
                    kept_sizes.insert(w);
                }
            }
        }
    }

    if new_dir.entries().is_empty() {
        return raw_bytes.to_vec();
    }

    let mut out = Vec::new();
    if new_dir.write(&mut out).is_ok() && out.len() < raw_bytes.len() {
        out
    } else {
        raw_bytes.to_vec()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PakStats {
    pub count: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Compresses every `.ico` of `source_folder` into a single pack.
/// `progress(done, total)` is called as icons are processed. The pack is
/// written to a temporary file then renamed, so a failure never leaves a
/// truncated pack behind.
pub fn build_compressed_pak(
    source_folder: &str,
    pak_output_path: &str,
    progress: &dyn Fn(usize, usize),
) -> Result<PakStats, String> {
    let entries = fs::read_dir(source_folder).map_err(|e| e.to_string())?;
    let mut raw_files = Vec::new();
    for p in entries.flatten().map(|e| e.path()) {
        let is_ico = p
            .extension()
            .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("ico"));
        if p.is_file() && is_ico {
            if let Some(stem) = p.file_stem() {
                raw_files.push((stem.to_string_lossy().to_string(), p));
            }
        }
    }
    raw_files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut compressed_blobs = Vec::new();
    let mut total_before = 0u64;
    let mut total_after = 0u64;

    let total = raw_files.len();
    progress(0, total);
    for (i, (name, path)) in raw_files.into_iter().enumerate() {
        if let Ok(buf) = fs::read(&path) {
            total_before += buf.len() as u64;
            let compressed = compress_ico_bytes(&buf);
            total_after += compressed.len() as u64;
            compressed_blobs.push((name, compressed));
        }
        progress(i + 1, total);
    }

    let mut temp_entries = Vec::new();
    let mut current_rel_offset = 0u64;
    for (name, blob) in &compressed_blobs {
        let len = blob.len() as u64;
        temp_entries.push(PakEntry {
            name: name.clone(),
            offset: current_rel_offset,
            length: len,
        });
        current_rel_offset += len;
    }

    let dummy_json = serde_json::to_vec(&PakIndex {
        entries: temp_entries.clone(),
    })
    .map_err(|e| e.to_string())?;

    let header_and_index_size = 12 + dummy_json.len() as u64 + 1024;
    for entry in &mut temp_entries {
        entry.offset += header_and_index_size;
    }

    let final_index_bytes = serde_json::to_vec(&PakIndex {
        entries: temp_entries,
    })
    .map_err(|e| e.to_string())?;

    if (12 + final_index_bytes.len() as u64) > header_and_index_size {
        return Err("gallery.pak index overflow".to_string());
    }

    let out_path = Path::new(pak_output_path);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let tmp_path = out_path.with_extension("pak.tmp");
    {
        let mut out_file = File::create(&tmp_path).map_err(|e| e.to_string())?;
        out_file.write_all(b"ICOP").map_err(|e| e.to_string())?;
        out_file
            .write_all(&(final_index_bytes.len() as u64).to_le_bytes())
            .map_err(|e| e.to_string())?;
        out_file
            .write_all(&final_index_bytes)
            .map_err(|e| e.to_string())?;
        let padding = header_and_index_size as usize - (12 + final_index_bytes.len());
        out_file
            .write_all(&vec![0u8; padding])
            .map_err(|e| e.to_string())?;
        for (_, blob) in &compressed_blobs {
            out_file.write_all(blob).map_err(|e| e.to_string())?;
        }
    }
    fs::rename(&tmp_path, out_path)
        .map_err(|e| format!("Cannot replace {}: {}", out_path.display(), e))?;

    Ok(PakStats {
        count: compressed_blobs.len(),
        bytes_before: total_before,
        bytes_after: total_after,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::time::{Duration, Instant};

    #[test]
    fn an_icon_name_has_the_category_that_shows_it() {
        assert_eq!(IconCategory::of("folder-src"), IconCategory::FoldersOnly);
        assert_eq!(IconCategory::of("folder"), IconCategory::FoldersOnly);
        assert_eq!(IconCategory::of("Rust_Light"), IconCategory::LightVariants);
        assert_eq!(IconCategory::of("rust.clone"), IconCategory::Clones);
        assert_eq!(IconCategory::of("rust"), IconCategory::FilesOnly);
        assert_eq!(IconCategory::of(""), IconCategory::All);
        assert_eq!(IconCategory::of("  "), IconCategory::All);
    }

    #[test]
    fn the_category_of_a_name_always_lists_that_name() {
        let names = ["folder-src", "folder", "rust_light", "rust.clone", "rust"];
        let lib = IconLibrary {
            names: names.iter().map(|n| n.to_string()).collect(),
            ..Default::default()
        };
        for name in names {
            let shown = lib.filter_names("", IconCategory::of(name));
            assert!(shown.iter().any(|n| n == name), "{name} not in {shown:?}");
        }
    }

    /// Temp folder with a few real multi-size .ico files, removed on drop.
    struct IconFixture {
        dir: PathBuf,
    }

    impl IconFixture {
        fn new(names: &[&str]) -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "wintune-gallery-{}-{}",
                std::process::id(),
                nanos
            ));
            fs::create_dir_all(dir.join("src")).unwrap();
            fs::create_dir_all(dir.join("empty")).unwrap();
            for (i, name) in names.iter().enumerate() {
                Self::write_icon(&dir.join("src").join(format!("{}.ico", name)), i);
            }
            Self { dir }
        }

        fn write_icon(path: &Path, seed: usize) {
            let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
            for size in [16u32, 32, 48] {
                let rgba = vec![(seed * 40) as u8; (size * size * 4) as usize];
                let img = ico::IconImage::from_rgba_data(size, size, rgba);
                icon.add_entry(ico::IconDirEntry::encode(&img).unwrap());
            }
            icon.write(File::create(path).unwrap()).unwrap();
        }

        fn src(&self) -> String {
            self.dir.join("src").to_string_lossy().into_owned()
        }

        fn empty(&self) -> String {
            self.dir.join("empty").to_string_lossy().into_owned()
        }

        fn pak(&self) -> String {
            self.dir.join("gallery.pak").to_string_lossy().into_owned()
        }
    }

    impl Drop for IconFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    fn no_progress(_: usize, _: usize) {}

    fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(v) = probe() {
                return Some(v);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    }

    #[test]
    fn rebuilding_the_pak_while_it_is_loaded_succeeds() {
        let fx = IconFixture::new(&["rust", "python"]);
        build_compressed_pak(&fx.src(), &fx.pak(), &no_progress).unwrap();
        let mut gallery = IconGallery::default();
        gallery.reload(&fx.src(), &fx.pak());

        let rebuilt = build_compressed_pak(&fx.src(), &fx.pak(), &no_progress);

        assert!(rebuilt.is_ok(), "{:?}", rebuilt);
    }

    #[test]
    fn pak_roundtrip_serves_every_icon() {
        let fx = IconFixture::new(&["rust", "Python"]);
        let stats = build_compressed_pak(&fx.src(), &fx.pak(), &no_progress).unwrap();
        assert_eq!(stats.count, 2);

        let lib = IconLibrary::load(&fx.empty(), &fx.pak());

        assert_eq!(lib.names(), ["Python".to_string(), "rust".to_string()]);
        for name in lib.names() {
            let bytes = lib.get_icon_bytes(name).unwrap();
            let dir = ico::IconDir::read(Cursor::new(bytes)).unwrap();
            assert_eq!(dir.entries().len(), 3);
        }
    }

    #[test]
    fn folder_icons_missing_from_the_pak_are_listed_too() {
        let fx = IconFixture::new(&["rust"]);
        build_compressed_pak(&fx.src(), &fx.pak(), &no_progress).unwrap();
        IconFixture::write_icon(&fx.dir.join("src").join("zig.ico"), 3);

        let lib = IconLibrary::load(&fx.src(), &fx.pak());

        assert_eq!(lib.names(), ["rust".to_string(), "zig".to_string()]);
        assert!(lib.get_icon_bytes("zig").is_some());
    }

    #[test]
    fn build_reports_progress_up_to_the_icon_count() {
        let fx = IconFixture::new(&["a", "b", "c"]);
        let calls = RefCell::new(Vec::new());

        build_compressed_pak(&fx.src(), &fx.pak(), &|d, t| {
            calls.borrow_mut().push((d, t))
        })
        .unwrap();

        let calls = calls.into_inner();
        assert_eq!(calls.last(), Some(&(3, 3)), "{:?}", calls);
        assert!(calls.windows(2).all(|w| w[0].0 <= w[1].0), "{:?}", calls);
    }

    #[test]
    fn a_pak_with_an_absurd_index_length_is_ignored_without_panicking() {
        let fx = IconFixture::new(&[]);
        let mut bytes = b"ICOP".to_vec();
        bytes.extend(u64::MAX.to_le_bytes());
        bytes.extend(b"{}");
        fs::write(fx.pak(), bytes).unwrap();

        let lib = IconLibrary::load(&fx.empty(), &fx.pak());

        assert!(lib.names().is_empty());
    }

    #[test]
    fn a_pak_entry_pointing_past_the_end_is_ignored_without_panicking() {
        let fx = IconFixture::new(&[]);
        let index = br#"{"entries":[{"name":"evil","offset":18446744073709551610,"length":100}]}"#;
        let mut bytes = b"ICOP".to_vec();
        bytes.extend((index.len() as u64).to_le_bytes());
        bytes.extend(index);
        fs::write(fx.pak(), bytes).unwrap();

        let lib = IconLibrary::load(&fx.empty(), &fx.pak());

        assert!(lib.get_icon_bytes("evil").is_none());
    }

    #[test]
    fn library_can_be_shared_with_worker_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<IconLibrary>();
        assert_send_sync::<IconTextureCache>();
    }

    #[test]
    fn textures_are_decoded_in_the_background() {
        let fx = IconFixture::new(&["rust"]);
        let lib = Arc::new(IconLibrary::load(&fx.src(), ""));
        let cache = IconTextureCache::new();
        let ctx = Context::default();

        assert!(
            cache.get(&ctx, &lib, "rust").is_none(),
            "first call must not block"
        );
        let tex = wait_for(|| cache.get(&ctx, &lib, "rust"));

        assert_eq!(tex.expect("texture never decoded").size(), [48, 48]);
    }

    #[test]
    fn unknown_icons_end_up_marked_as_failed() {
        let fx = IconFixture::new(&[]);
        let lib = Arc::new(IconLibrary::load(&fx.src(), ""));
        let cache = IconTextureCache::new();
        let ctx = Context::default();

        cache.get(&ctx, &lib, "nope");

        assert!(wait_for(|| cache.is_failed("nope").then_some(())).is_some());
    }

    #[test]
    fn clearing_the_cache_drops_decoded_textures() {
        let fx = IconFixture::new(&["rust"]);
        let lib = Arc::new(IconLibrary::load(&fx.src(), ""));
        let cache = IconTextureCache::new();
        let ctx = Context::default();
        cache.get(&ctx, &lib, "rust");
        let _decoded = wait_for(|| cache.get(&ctx, &lib, "rust")).expect("decoded");

        cache.clear();

        assert!(cache.get(&ctx, &lib, "rust").is_none());
    }
}
