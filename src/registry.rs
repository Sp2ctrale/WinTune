use crate::detector::{build_editor_command, get_loaded_user_sids, has_wsl_distribution};
use crate::gallery::IconGallery;
use crate::models::{
    BackupStore, DoubleClickMode, ExtensionBackup, ExtensionConfig, RegistryKeySnapshot,
    UserOverrideSnapshot,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const MACHINE_CLASSES: &str = "SOFTWARE\\Classes";
const USER_CLASSES: &str = "Software\\Classes";
const USER_FILE_EXTS: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts";

pub fn backup_store_path() -> String {
    let program_data = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".to_string());
    format!("{}\\FileIconsManager\\backups.json", program_data)
}

pub fn load_backup_store() -> BackupStore {
    let path = backup_store_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(store) = serde_json::from_str::<BackupStore>(&data) {
            return store;
        }
    }
    BackupStore::default()
}

pub fn save_backup_store(store: &BackupStore) -> Result<(), String> {
    let path = backup_store_path();
    if let Some(parent) = Path::new(&path).parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

fn snapshot_reg_key(root: &RegKey, subkey_path: &str) -> RegistryKeySnapshot {
    let key = match root.open_subkey_with_flags(subkey_path, KEY_READ) {
        Ok(k) => k,
        Err(_) => {
            return RegistryKeySnapshot {
                exists: false,
                default_value: None,
                values: BTreeMap::new(),
                subkeys: BTreeMap::new(),
            }
        }
    };

    let default_value: Option<String> = key.get_value("").ok();
    let mut values = BTreeMap::new();

    for val_res in key.enum_values() {
        if let Ok((name, reg_val)) = val_res {
            if !name.is_empty() {
                let kind_u32 = reg_val.vtype as u32;
                values.insert(name, (kind_u32, reg_val.bytes));
            }
        }
    }

    let mut subkeys = BTreeMap::new();
    for sub_res in key.enum_keys() {
        if let Ok(sub_name) = sub_res {
            let child_path = format!("{}\\{}", subkey_path, sub_name);
            subkeys.insert(sub_name, snapshot_reg_key(root, &child_path));
        }
    }

    RegistryKeySnapshot {
        exists: true,
        default_value,
        values,
        subkeys,
    }
}

fn restore_reg_key(root: &RegKey, subkey_path: &str, snapshot: &RegistryKeySnapshot) -> Result<(), String> {
    let _ = root.delete_subkey_all(subkey_path);
    if !snapshot.exists {
        return Ok(());
    }

    let (key, _) = root
        .create_subkey(subkey_path)
        .map_err(|e| format!("Création clé impossible {} : {}", subkey_path, e))?;

    if let Some(def) = &snapshot.default_value {
        let _ = key.set_value("", def);
    }

    for (name, (kind_u32, bytes)) in &snapshot.values {
        let vtype = match *kind_u32 {
            1 => REG_SZ,
            2 => REG_EXPAND_SZ,
            3 => REG_BINARY,
            4 => REG_DWORD,
            7 => REG_MULTI_SZ,
            11 => REG_QWORD,
            _ => REG_BINARY,
        };
        let raw_val = RegValue {
            bytes: bytes.clone(),
            vtype,
        };
        let _ = key.set_raw_value(name, &raw_val);
    }

    for (sub_name, sub_snap) in &snapshot.subkeys {
        let child_path = format!("{}\\{}", subkey_path, sub_name);
        let _ = restore_reg_key(root, &child_path, sub_snap);
    }

    Ok(())
}

fn create_initial_backup_if_needed(
    store: &mut BackupStore,
    ext_cfg: &ExtensionConfig,
    icon_dest_folder: &str,
) {
    if store.backups.contains_key(&ext_cfg.extension) {
        return;
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hku = RegKey::predef(HKEY_USERS);
    let ext_key_path = format!("{}\\{}", MACHINE_CLASSES, ext_cfg.extension);
    let hklm_ext_snap = snapshot_reg_key(&hklm, &ext_key_path);

    let prev_prog_id = hklm_ext_snap
        .default_value
        .clone()
        .filter(|p| !p.is_empty() && p != &ext_cfg.prog_id)
        .map(|prog_id| {
            let prog_path = format!("{}\\{}", MACHINE_CLASSES, prog_id);
            let snap = snapshot_reg_key(&hklm, &prog_path);
            (prog_id, snap)
        });

    let mut user_overrides = Vec::new();
    for sid in get_loaded_user_sids() {
        let u_classes_path = format!("{}\\{}\\{}", sid, USER_CLASSES, ext_cfg.extension);
        let u_file_exts_path = format!("{}\\{}\\{}", sid, USER_FILE_EXTS, ext_cfg.extension);
        user_overrides.push(UserOverrideSnapshot {
            sid,
            user_classes_ext: snapshot_reg_key(&hku, &u_classes_path),
            user_file_exts: snapshot_reg_key(&hku, &u_file_exts_path),
        });
    }

    let icon_installed_path = Path::new(icon_dest_folder)
        .join(format!("{}.ico", ext_cfg.icon_name))
        .to_string_lossy()
        .to_string();

    let backup = ExtensionBackup {
        extension: ext_cfg.extension.clone(),
        custom_prog_id: ext_cfg.prog_id.clone(),
        icon_installed_path,
        timestamp: "Snapshot initial".to_string(),
        hklm_extension_key: hklm_ext_snap,
        hklm_previous_prog_id: prev_prog_id,
        user_overrides,
    };

    store.backups.insert(ext_cfg.extension.clone(), backup);
}

fn set_expand_sz(key: &RegKey, name: &str, value: &str) -> Result<(), String> {
    let mut utf16: Vec<u16> = value.encode_utf16().collect();
    utf16.push(0);
    let bytes: Vec<u8> = utf16.iter().flat_map(|w| w.to_le_bytes()).collect();
    let reg_val = RegValue {
        bytes,
        vtype: REG_EXPAND_SZ,
    };
    key.set_raw_value(name, &reg_val)
        .map_err(|e| e.to_string())
}

pub fn apply_extensions(
    extensions: &[ExtensionConfig],
    code_exe: &str,
    icon_dest_folder: &str,
    gallery: &IconGallery,
) -> (usize, Vec<String>) {
    let mut store = load_backup_store();
    let editor_command = build_editor_command(code_exe);
    let wsl_available = has_wsl_distribution();
    let sids = get_loaded_user_sids();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hku = RegKey::predef(HKEY_USERS);

    let mut success_count = 0;
    let mut errors = Vec::new();

    for ext_cfg in extensions.iter().filter(|e| e.enabled) {
        create_initial_backup_if_needed(&mut store, ext_cfg, icon_dest_folder);

        let icon_path = match gallery.export_icon_to(&ext_cfg.icon_name, icon_dest_folder) {
            Ok(p) => p,
            Err(err) => {
                errors.push(format!("{} ({})", ext_cfg.extension, err));
                continue;
            }
        };

        for sid in &sids {
            if ext_cfg.reset_user_choice {
                let u_file_ext = format!("{}\\{}\\{}", sid, USER_FILE_EXTS, ext_cfg.extension);
                let _ = hku.delete_subkey_all(&u_file_ext);
            }
            if ext_cfg.reset_existing {
                let u_class_ext = format!("{}\\{}\\{}", sid, USER_CLASSES, ext_cfg.extension);
                let _ = hku.delete_subkey_all(&u_class_ext);
            }
        }

        let ext_key_path = format!("{}\\{}", MACHINE_CLASSES, ext_cfg.extension);
        let prog_id_path = format!("{}\\{}", MACHINE_CLASSES, ext_cfg.prog_id);

        let previous_prog_id: Option<String> = hklm
            .open_subkey_with_flags(&ext_key_path, KEY_READ)
            .ok()
            .and_then(|k| k.get_value("").ok())
            .filter(|v: &String| !v.is_empty() && v != &ext_cfg.prog_id);

        let _ = hklm.delete_subkey_all(&prog_id_path);

        let res = (|| -> Result<(), String> {
            let (prog_key, _) = hklm.create_subkey(&prog_id_path).map_err(|e| e.to_string())?;
            prog_key
                .set_value("", &ext_cfg.description)
                .map_err(|e| e.to_string())?;

            let (icon_key, _) = hklm
                .create_subkey(format!("{}\\DefaultIcon", prog_id_path))
                .map_err(|e| e.to_string())?;
            icon_key
                .set_value("", &icon_path)
                .map_err(|e| e.to_string())?;

            let use_launcher = ext_cfg.mode == DoubleClickMode::ExecuteScript
                && ext_cfg.launcher_command.is_some()
                && !(ext_cfg.prog_id == "CustomShellFile" && !wsl_available);

            if use_launcher {
                let launcher = ext_cfg.launcher_command.as_ref().unwrap();
                let (open_cmd_key, _) = hklm
                    .create_subkey(format!("{}\\shell\\open\\command", prog_id_path))
                    .map_err(|e| e.to_string())?;
                open_cmd_key
                    .set_value("", launcher)
                    .map_err(|e| e.to_string())?;

                let (edit_key, _) = hklm
                    .create_subkey(format!("{}\\shell\\edit", prog_id_path))
                    .map_err(|e| e.to_string())?;
                edit_key
                    .set_value("", &"Modifier")
                    .map_err(|e| e.to_string())?;

                let (edit_cmd_key, _) = hklm
                    .create_subkey(format!("{}\\shell\\edit\\command", prog_id_path))
                    .map_err(|e| e.to_string())?;
                set_expand_sz(&edit_cmd_key, "", &editor_command)?;
            } else {
                let (open_cmd_key, _) = hklm
                    .create_subkey(format!("{}\\shell\\open\\command", prog_id_path))
                    .map_err(|e| e.to_string())?;
                set_expand_sz(&open_cmd_key, "", &editor_command)?;
            }

            if let Some(elevated) = &ext_cfg.elevated_command {
                if !elevated.trim().is_empty() {
                    let (runas_key, _) = hklm
                        .create_subkey(format!("{}\\shell\\runas\\command", prog_id_path))
                        .map_err(|e| e.to_string())?;
                    runas_key
                        .set_value("", elevated)
                        .map_err(|e| e.to_string())?;
                }
            }

            for verb in &ext_cfg.extra_verbs {
                if !verb.verb_key.trim().is_empty() && !verb.command.trim().is_empty() {
                    let (v_key, _) = hklm
                        .create_subkey(format!("{}\\shell\\{}", prog_id_path, verb.verb_key))
                        .map_err(|e| e.to_string())?;
                    v_key
                        .set_value("", &verb.menu_label)
                        .map_err(|e| e.to_string())?;
                    let (v_cmd_key, _) = hklm
                        .create_subkey(format!(
                            "{}\\shell\\{}\\command",
                            prog_id_path, verb.verb_key
                        ))
                        .map_err(|e| e.to_string())?;
                    set_expand_sz(&v_cmd_key, "", &verb.command)?;
                }
            }

            if ext_cfg.reset_existing && !ext_cfg.preserve_existing_properties {
                let _ = hklm.delete_subkey_all(&ext_key_path);
            }

            let (ext_key, _) = hklm.create_subkey(&ext_key_path).map_err(|e| e.to_string())?;
            ext_key
                .set_value("", &ext_cfg.prog_id)
                .map_err(|e| e.to_string())?;

            if !ext_cfg.perceived_type.trim().is_empty() {
                let _ = ext_key.set_value("PerceivedType", &ext_cfg.perceived_type);
            }
            if !ext_cfg.content_type.trim().is_empty() {
                let _ = ext_key.set_value("Content Type", &ext_cfg.content_type);
            }

            if ext_cfg.preserve_existing_properties {
                if let Some(prev) = previous_prog_id {
                    if let Ok((owp_key, _)) =
                        hklm.create_subkey(format!("{}\\OpenWithProgids", ext_key_path))
                    {
                        let _ = owp_key.set_value(&prev, &"");
                    }
                }
            }

            Ok(())
        })();

        match res {
            Ok(_) => success_count += 1,
            Err(e) => errors.push(format!("{} : {}", ext_cfg.extension, e)),
        }
    }

    let _ = save_backup_store(&store);
    notify_shell_associations();
    (success_count, errors)
}

pub fn rollback_selected_extensions(selected_extensions: &[String]) -> (usize, Vec<String>) {
    let mut store = load_backup_store();
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hku = RegKey::predef(HKEY_USERS);

    let mut restored_count = 0;
    let mut errors = Vec::new();

    for ext in selected_extensions {
        if let Some(backup) = store.backups.get(ext).cloned() {
            let ext_key_path = format!("{}\\{}", MACHINE_CLASSES, backup.extension);
            let custom_prog_path = format!("{}\\{}", MACHINE_CLASSES, backup.custom_prog_id);

            let _ = hklm.delete_subkey_all(&custom_prog_path);

            if let Err(e) = restore_reg_key(&hklm, &ext_key_path, &backup.hklm_extension_key) {
                errors.push(format!("{} : {}", ext, e));
                continue;
            }

            if let Some((prev_id, prev_snap)) = &backup.hklm_previous_prog_id {
                let prev_path = format!("{}\\{}", MACHINE_CLASSES, prev_id);
                let _ = restore_reg_key(&hklm, &prev_path, prev_snap);
            }

            for u_snap in &backup.user_overrides {
                let u_classes_path = format!("{}\\{}\\{}", u_snap.sid, USER_CLASSES, backup.extension);
                let u_file_exts_path =
                    format!("{}\\{}\\{}", u_snap.sid, USER_FILE_EXTS, backup.extension);
                let _ = restore_reg_key(&hku, &u_classes_path, &u_snap.user_classes_ext);
                let _ = restore_reg_key(&hku, &u_file_exts_path, &u_snap.user_file_exts);
            }

            store.backups.remove(ext);
            restored_count += 1;
        }
    }

    let _ = save_backup_store(&store);
    notify_shell_associations();
    (restored_count, errors)
}

pub fn notify_shell_associations() {
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}
