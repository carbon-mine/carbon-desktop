// SPDX-License-Identifier: MPL-2.0
use super::{storage, Profile};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    path::Path,
};
use uuid::Uuid;

const INVALID: &str =
    "Enter a DNS name, IPv4 address, or bracketed IPv6 address. A port must be from 1 to 65535.";

/// A normalized Minecraft Java destination. Parsing performs no DNS lookup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ServerAddress(String);

impl ServerAddress {
    pub fn parse_optional(input: &str) -> Result<Option<Self>, String> {
        if input.trim_matches(' ').is_empty() {
            Ok(None)
        } else {
            Self::parse(input).map(Some)
        }
    }

    pub fn parse(input: &str) -> Result<Self, String> {
        let address = input.trim_matches(' ');
        if address.is_empty() || address.len() > 260 || !address.is_ascii() {
            return Err(INVALID.into());
        }
        let (host, port) = match address.strip_prefix('[') {
            Some(bracketed) => {
                let (host, tail) = bracketed.split_once(']').ok_or(INVALID)?;
                let ipv6 = host.parse::<Ipv6Addr>().map_err(|_| INVALID)?;
                let port = if tail.is_empty() {
                    None
                } else {
                    Some(tail.strip_prefix(':').ok_or(INVALID)?)
                };
                (format!("[{ipv6}]"), port)
            }
            None => {
                let (host, port) = address
                    .split_once(':')
                    .map_or((address, None), |(host, port)| (host, Some(port)));
                (normalize_host(host).ok_or(INVALID)?, port)
            }
        };
        if let Some(port) = port {
            let port = port
                .bytes()
                .all(|byte| byte.is_ascii_digit())
                .then(|| port.parse::<u16>().ok())
                .flatten()
                .filter(|port| *port > 0)
                .ok_or(INVALID)?;
            Ok(Self(format!("{host}:{port}")))
        } else {
            Ok(Self(host))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ServerAddress {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<ServerAddress> for String {
    fn from(value: ServerAddress) -> Self {
        value.0
    }
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

fn normalize_host(host: &str) -> Option<String> {
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return Some(ip.to_string());
    }
    let labels = host.strip_suffix('.').unwrap_or(host);
    if labels.len() > 253
        || labels
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return None;
    }
    let valid = labels.split('.').all(|label| {
        let bytes = label.as_bytes();
        bytes.len() <= 63
            && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
            && bytes
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
    });
    valid.then(|| host.to_ascii_lowercase())
}

pub fn server_profile(root: &Path, id: Uuid) -> Result<Profile, String> {
    let (_lock, library) = storage::open(root)?;
    Ok(library.profiles[library.position(id)?].clone())
}

pub struct ServerBindingUpdate {
    pub profile: Profile,
    pub changed: bool,
}

impl ServerBindingUpdate {
    pub fn message(&self) -> &'static str {
        if !self.changed {
            "No server address changes."
        } else if self.profile.server_address.is_some() {
            "Server address saved."
        } else {
            "Server address removed."
        }
    }
}

/// Transact only the binding, using the value observed when the editor opened.
pub fn set_server_address(
    root: &Path,
    id: Uuid,
    expected: Option<&ServerAddress>,
    address: Option<ServerAddress>,
) -> Result<ServerBindingUpdate, String> {
    let (_lock, mut library) = storage::open(root)?;
    let position = library.position(id)?;
    let profile = &mut library.profiles[position];
    if profile.server_address.as_ref() != expected {
        return Err(
            "The server address changed in another window. Reload this instance and try again."
                .into(),
        );
    }
    if profile.server_address == address {
        return Ok(ServerBindingUpdate {
            profile: profile.clone(),
            changed: false,
        });
    }
    profile.server_address = address;
    let updated = profile.clone();
    storage::save(&root.join("personal"), &library)?;
    Ok(ServerBindingUpdate {
        profile: updated,
        changed: true,
    })
}

/// A local explanation only. It never asks the account provider, server, or network.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerJoinReadiness {
    NoServer,
    UnqualifiedPack,
    ProtectedLaunchUnavailable,
}

impl ServerJoinReadiness {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NoServer => "Set a server address to keep it with this instance.",
            Self::UnqualifiedPack => "Join server is unavailable. This local pack still needs supported client files or a qualified loader.",
            Self::ProtectedLaunchUnavailable => "Join server is unavailable while protected Minecraft launch is in development.",
        }
    }
}

impl Profile {
    pub fn server_join_readiness(&self) -> ServerJoinReadiness {
        if self.server_address.is_none() {
            ServerJoinReadiness::NoServer
        } else if self
            .imported_pack
            .as_ref()
            .is_some_and(|pack| pack.loader.is_some() || pack.pending_files != 0)
        {
            ServerJoinReadiness::UnqualifiedPack
        } else {
            ServerJoinReadiness::ProtectedLaunchUnavailable
        }
    }
}
