//! Uniform Resource Names for Artisan platform resources.
//!
//! See `RESOURCE_TAXONOMY.md` §5 at the repository root for the normative
//! grammar. Format: `urn:artisan:<resource-type>:<id>`, where composite ids
//! (currently only [`ResourceType::Secret`]) join their parts with `/` inside
//! the id segment.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The resource types a [`Urn`] can name. Deliberately excludes `Runner` and
/// `Repo` -- see `RESOURCE_TAXONOMY.md` §3.2/§3.10 for why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Organization,
    User,
    Node,
    Project,
    Instance,
    Domain,
    Secret,
    Environment,
    Session,
    Vm,
    Invite,
}

impl ResourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceType::Organization => "organization",
            ResourceType::User => "user",
            ResourceType::Node => "node",
            ResourceType::Project => "project",
            ResourceType::Instance => "instance",
            ResourceType::Domain => "domain",
            ResourceType::Secret => "secret",
            ResourceType::Environment => "environment",
            ResourceType::Session => "session",
            ResourceType::Vm => "vm",
            ResourceType::Invite => "invite",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "organization" => ResourceType::Organization,
            "user" => ResourceType::User,
            "node" => ResourceType::Node,
            "project" => ResourceType::Project,
            "instance" => ResourceType::Instance,
            "domain" => ResourceType::Domain,
            "secret" => ResourceType::Secret,
            "environment" => ResourceType::Environment,
            "session" => ResourceType::Session,
            "vm" => ResourceType::Vm,
            "invite" => ResourceType::Invite,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod resource_type_tests {
    use super::ResourceType;

    #[test]
    fn every_resource_type_round_trips_through_as_str_and_from_str() {
        for resource_type in [
            ResourceType::Organization,
            ResourceType::User,
            ResourceType::Node,
            ResourceType::Project,
            ResourceType::Instance,
            ResourceType::Domain,
            ResourceType::Secret,
            ResourceType::Environment,
            ResourceType::Session,
            ResourceType::Vm,
            ResourceType::Invite,
        ] {
            assert_eq!(
                ResourceType::from_str(resource_type.as_str()),
                Some(resource_type)
            );
        }
    }

    #[test]
    fn from_str_rejects_an_unrecognized_resource_type() {
        assert_eq!(ResourceType::from_str("runner"), None); // retired name -- see RESOURCE_TAXONOMY.md
        assert_eq!(ResourceType::from_str(""), None);
    }
}

/// Error returned by [`Urn::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrnParseError {
    /// Missing the `urn:artisan:` prefix, or too few `:`-separated segments.
    Malformed(String),
    /// The resource-type segment isn't one of [`ResourceType`]'s known values.
    UnknownResourceType(String),
}

impl fmt::Display for UrnParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrnParseError::Malformed(s) => write!(f, "malformed URN: {:?}", s),
            UrnParseError::UnknownResourceType(s) => {
                write!(f, "unknown URN resource type: {:?}", s)
            }
        }
    }
}

impl std::error::Error for UrnParseError {}

/// A parsed `urn:artisan:<resource-type>:<id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Urn {
    resource_type: ResourceType,
    id: String,
}

const URN_PREFIX: &str = "urn:artisan:";

impl Urn {
    pub fn new(resource_type: ResourceType, id: impl Into<String>) -> Self {
        Self {
            resource_type,
            id: id.into(),
        }
    }

    pub fn resource_type(&self) -> ResourceType {
        self.resource_type
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Parses a URN string. Splits on the first three `:` only, so a `/`
    /// inside a composite id (e.g. a [`ResourceType::Secret`] ref) never
    /// collides with the grammar.
    pub fn parse(s: &str) -> Result<Self, UrnParseError> {
        let rest = s
            .strip_prefix(URN_PREFIX)
            .ok_or_else(|| UrnParseError::Malformed(s.to_owned()))?;
        let (type_segment, id) = rest
            .split_once(':')
            .ok_or_else(|| UrnParseError::Malformed(s.to_owned()))?;
        if id.is_empty() {
            return Err(UrnParseError::Malformed(s.to_owned()));
        }
        let resource_type = ResourceType::from_str(type_segment)
            .ok_or_else(|| UrnParseError::UnknownResourceType(type_segment.to_owned()))?;
        Ok(Self {
            resource_type,
            id: id.to_owned(),
        })
    }
}

impl fmt::Display for Urn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{URN_PREFIX}{}:{}", self.resource_type.as_str(), self.id)
    }
}
