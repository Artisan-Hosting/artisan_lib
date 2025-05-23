use std::{
    fs,
    os::unix::fs::{chown, PermissionsExt},
};

use dusa_collection_utils::{
    core::errors::{ErrorArrayItem, Errors},
    core::types::pathtype::PathType,
};
use users::{Groups, Users, UsersCache};
use walkdir::WalkDir;

/// Getting the current uid (uid, gid)
pub fn get_id(user: &str) -> Result<(u32, u32), ErrorArrayItem> {
    let user_cache: UsersCache = UsersCache::new();

    let uid_result: Result<u32, ErrorArrayItem> =
        match user_cache.get_user_by_name(&format! {"{}", user}) {
            Some(d) => Ok(d.uid()),
            None => Err(ErrorArrayItem::new(
                Errors::GeneralError,
                String::from("The requested user doesn't exist"),
            )),
        };

    let gid_result: Result<u32, ErrorArrayItem> =
        match user_cache.get_group_by_name(&format! {"{}", user}) {
            Some(d) => Ok(d.gid()),
            None => Err(ErrorArrayItem::new(
                Errors::GeneralError,
                String::from("The requested group doesn't exist"),
            )),
        };

    let ais_uid = uid_result?;
    let ais_gid = gid_result?;

    Ok((ais_uid, ais_gid))
}

pub fn set_file_ownership(path: &PathType, uid: u32, gid: u32) -> Result<(), ErrorArrayItem> {
    let path_buf = path.to_path_buf();

    if path_buf.is_dir() {
        // Use WalkDir to recursively change ownership
        for entry in WalkDir::new(&path_buf).into_iter().filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if let Err(err) = chown(entry_path, Some(uid), Some(gid)) {
                return Err(ErrorArrayItem::from(err));
            }
        }
    } else {
        // Change ownership of the single file
        if let Err(err) = chown(&path_buf, Some(uid), Some(gid)) {
            return Err(ErrorArrayItem::from(err));
        }
    }

    Ok(())
}

pub fn set_file_permission(path: PathType, permission: u32) -> Result<(), ErrorArrayItem> {
    // Changing the permissions the socket
    let path_metadata = match fs::metadata(path.clone()) {
        Ok(d) => d,
        Err(e) => return Err(ErrorArrayItem::from(e)),
    };

    let permission_string: String = format!("0o{}", permission);
    let permission_int: u32 = permission_string
        .parse::<u32>()
        .map_err(|e| ErrorArrayItem::from(e))?;

    let mut permissions = path_metadata.permissions();
    permissions.set_mode(permission_int); // Set desired permissions

    if let Err(err) = fs::set_permissions(path.clone(), permissions) {
        return Err(ErrorArrayItem::from(err));
    }

    Ok(())
}
