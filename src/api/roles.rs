use serde::{Deserialize, Serialize};
use std::ops::BitOr;


#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Copy)]
pub enum Role {
    Super,
    Admin,
    Controller,
    Viewer,
    Audit,
    None,
}

impl BitOr for Role {
    type Output = bool;

    fn bitor(self, rhs: Role) -> Self::Output {
        matches!(self, Role::Admin)
            || matches!(rhs, Role::Admin)
            || matches!(self, Role::Controller)
            || matches!(rhs, Role::Controller)
            || matches!(self, Role::Viewer)
            || matches!(rhs, Role::Viewer)
            || matches!(self, Role::Audit)
            || matches!(rhs, Role::Audit)
            || matches!(self, Role::Super)
            || matches!(rhs, Role::Super)
    }
}

impl Role {
    pub fn from_str(role: &str) -> Self {
        match role {
            "admin" => Self::Admin,
            "controller" => Self::Controller,
            "viewer" => Self::Viewer,
            "SUPER" => Self::Super,
            "audit" => Self::Audit,
            "none" => Self::None,
            _ => Self::None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Controller => "controller",
            Self::Viewer => "viewer",
            Self::Super => "SUPER",
            Self::Audit => "audit",
            Self::None => "none",
        }
    }
}

/// Determines if a given role level grants access to a required role.
///
/// # Arguments
/// * `current_role` - The role currently held by the user.
/// * `required_role` - The minimum role required for access.
///
/// # Returns
/// * `true` if `current_role` meets or exceeds `required_role`, otherwise `false`.
pub fn has_org_permission(current_role: Role, required_role: Role) -> bool {
    match (current_role, required_role) {
        (Role::Admin, _) => true,                   // Admins have full access
        (Role::Controller, Role::Viewer) => true,   // Controllers can view
        (Role::Controller, Role::Audit) => true,    // Controllers can audit
        (Role::Viewer, Role::Audit) => true,        // Viewers can audit
        _ if current_role == required_role => true, // Exact match of roles
        _ => false,                                 // Otherwise, no access
    }
}


// TODO add a super user. I SHOULD GET A NOTIFICATION AND HELLA LOGS WHEN SU EXECUTES A COMMAND
// TODO said super user shoud be able to do both user and admin only things
