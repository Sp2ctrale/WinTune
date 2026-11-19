//! Rollback: put extensions back to their pre-WinTune state.

use super::store::migrate_legacy_backup;
use super::target::AssocTarget;
use super::OperationReport;
use crate::models::BackupStore;
use crate::regsnap;

/// Puts each extension back to its pre-WinTune state. A snapshot is dropped
/// only once it has been fully restored; on any error it is kept so the
/// rollback can be retried. The caller saves the store and notifies the shell.
pub fn rollback_extensions(
    target: &AssocTarget,
    store: &mut BackupStore,
    extensions: &[String],
    progress: &dyn Fn(usize, usize, &str),
) -> OperationReport {
    let mut report = OperationReport::default();
    let total = extensions.len();
    progress(0, total, "Restoring");

    for (i, ext) in extensions.iter().enumerate() {
        progress(i, total, &format!("Restoring {}", ext));
        let Some(mut backup) = store.backups.get(ext).cloned() else {
            report.errors.push(format!("{}: no snapshot recorded", ext));
            continue;
        };
        migrate_legacy_backup(&mut backup);
        let mut errors = Vec::new();
        let mut signed_out = Vec::new();

        for user in &backup.user_overrides {
            if target.users_root.open_subkey(&user.sid).is_err() {
                signed_out.push(user.clone());
                continue;
            }
            let pairs = [
                (
                    target.user_classes_path(&user.sid, ext),
                    &user.user_classes_ext,
                ),
                (
                    target.user_file_exts_path(&user.sid, ext),
                    &user.user_file_exts,
                ),
            ];
            for (path, snap) in pairs {
                if let Err(e) = regsnap::restore_key(&target.users_root, &path, snap) {
                    errors.push(e);
                }
            }
        }

        if !backup.machine_restored {
            for (pid, snap) in &backup.prog_id_snapshots {
                if let Err(e) =
                    regsnap::restore_key(&target.machine_root, &target.class_path(pid), snap)
                {
                    errors.push(e);
                }
            }

            if let Err(e) = regsnap::restore_key(
                &target.machine_root,
                &target.class_path(ext),
                &backup.hklm_extension_key,
            ) {
                errors.push(e);
            }
        }

        if errors.is_empty() && signed_out.is_empty() {
            store.backups.remove(ext);
            report.succeeded.push(ext.clone());
        } else if errors.is_empty() {
            // Everything reachable is restored; keep only what belongs to
            // users who are signed out, to finish once they are back.
            let users: Vec<String> = signed_out.iter().map(|u| u.sid.clone()).collect();
            backup.user_overrides = signed_out;
            backup.machine_restored = true;
            store.backups.insert(ext.clone(), backup);
            report.warnings.push(format!(
                "{}: restored, except for signed-out user(s) {}; roll back again once they are signed in",
                ext,
                users.join(", ")
            ));
        } else {
            report.errors.push(format!(
                "{}: {} (snapshot kept, rollback can be retried)",
                ext,
                errors.join("; ")
            ));
        }
    }

    progress(total, total, "Rollback finished");
    report
}
