use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconCategory {
    All,
    FilesOnly,
    FoldersOnly,
    LightVariants,
    Clones,
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

pub struct IconGallery {
    pub names: Vec<String>,
    pub file_paths: HashMap<String, PathBuf>,
    pub pak_mmap: Option<Mmap>,
    pub pak_entries: HashMap<String, (u64, u64)>,
    pub textures: HashMap<String, TextureHandle>,
    pub failed_textures: std::collections::HashSet<String>,
}

impl Default for IconGallery {
    fn default() -> Self {
        Self {
            names: Vec::new(),
            file_paths: HashMap::new(),
            pak_mmap: None,
            pak_entries: HashMap::new(),
            textures: HashMap::new(),
            failed_textures: std::collections::HashSet::new(),
        }
    }
}

impl IconGallery {
    pub fn reload(&mut self, source_folder: &str, pak_path: &str) {
        self.names.clear();
        self.file_paths.clear();
        self.pak_mmap = None;
        self.pak_entries.clear();
        self.textures.clear();
        self.failed_textures.clear();

        if Path::new(pak_path).is_file() {
            let _ = self.load_pak(pak_path);
        }

        if let Ok(entries) = fs::read_dir(source_folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext.to_string_lossy().eq_ignore_ascii_case("ico") {
                            if let Some(stem) = path.file_stem() {
                                let name = stem.to_string_lossy().to_string();
                                self.file_paths.insert(name.clone(), path);
                                if !self.pak_entries.contains_key(&name) {
                                    self.names.push(name);
                                }
                            }
                        }
                    }
                }
            }
        }

        self.names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        self.names.dedup();
    }

    fn load_pak(&mut self, pak_path: &str) -> Result<(), String> {
        let file = File::open(pak_path).map_err(|e| e.to_string())?;
        let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };
        if mmap.len() < 12 || &mmap[0..4] != b"ICOP" {
            return Err("Format gallery.pak invalide".to_string());
        }
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&mmap[4..12]);
        let index_len = u64::from_le_bytes(len_bytes) as usize;
        if mmap.len() < 12 + index_len {
            return Err("Index gallery.pak corrompu".to_string());
        }
        let index: PakIndex =
            serde_json::from_slice(&mmap[12..12 + index_len]).map_err(|e| e.to_string())?;
        for entry in index.entries {
            self.pak_entries
                .insert(entry.name.clone(), (entry.offset, entry.length));
            self.names.push(entry.name);
        }
        self.pak_mmap = Some(mmap);
        Ok(())
    }

    pub fn get_icon_bytes(&self, name: &str) -> Option<Vec<u8>> {
        if let (Some(mmap), Some(&(offset, length))) = (&self.pak_mmap, self.pak_entries.get(name)) {
            let start = offset as usize;
            let end = start + length as usize;
            if end <= mmap.len() {
                return Some(mmap[start..end].to_vec());
            }
        }
        if let Some(path) = self.file_paths.get(name) {
            return fs::read(path).ok();
        }
        None
    }

    pub fn export_icon_to(&self, name: &str, dest_folder: &str) -> Result<String, String> {
        fs::create_dir_all(dest_folder).map_err(|e| e.to_string())?;
        let dest_path = Path::new(dest_folder).join(format!("{}.ico", name));
        if let Some(bytes) = self.get_icon_bytes(name) {
            fs::write(&dest_path, bytes).map_err(|e| e.to_string())?;
            Ok(dest_path.to_string_lossy().to_string())
        } else {
            Err(format!("Icône introuvable dans la galerie : {}", name))
        }
    }

    pub fn get_or_load_texture(&mut self, ctx: &Context, name: &str) -> Option<TextureHandle> {
        if let Some(tex) = self.textures.get(name) {
            return Some(tex.clone());
        }
        if self.failed_textures.contains(name) {
            return None;
        }
        let bytes = match self.get_icon_bytes(name) {
            Some(b) => b,
            None => {
                self.failed_textures.insert(name.to_string());
                return None;
            }
        };

        if let Some(color_image) = decode_ico_thumbnail(&bytes, 48) {
            let handle = ctx.load_texture(name.to_string(), color_image, TextureOptions::LINEAR);
            self.textures.insert(name.to_string(), handle.clone());
            Some(handle)
        } else {
            self.failed_textures.insert(name.to_string());
            None
        }
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
    let mut kept_sizes = std::collections::HashSet::new();

    for entry in icon_dir.entries() {
        let w = entry.width();
        let h = entry.height();
        if (w == 16 || w == 32 || w == 48 || w == 64 || w == 128 || w == 256) && !kept_sizes.contains(&w) {
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

pub fn build_compressed_pak(source_folder: &str, pak_output_path: &str) -> Result<(usize, u64, u64), String> {
    let entries = fs::read_dir(source_folder).map_err(|e| e.to_string())?;
    let mut raw_files = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(ext) = p.extension() {
                if ext.to_string_lossy().eq_ignore_ascii_case("ico") {
                    if let Some(stem) = p.file_stem() {
                        raw_files.push((stem.to_string_lossy().to_string(), p));
                    }
                }
            }
        }
    }
    raw_files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut compressed_blobs = Vec::new();
    let mut total_before = 0u64;
    let mut total_after = 0u64;

    for (name, path) in raw_files {
        let mut buf = Vec::new();
        if let Ok(mut f) = File::open(&path) {
            if f.read_to_end(&mut buf).is_ok() {
                total_before += buf.len() as u64;
                let compressed = compress_ico_bytes(&buf);
                total_after += compressed.len() as u64;
                compressed_blobs.push((name, compressed));
            }
        }
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
        return Err("Dépassement de taille d'index".to_string());
    }

    if let Some(parent) = Path::new(pak_output_path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut out_file = File::create(pak_output_path).map_err(|e| e.to_string())?;
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

    Ok((compressed_blobs.len(), total_before, total_after))
}
