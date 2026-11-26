// SPDX-License-Identifier: MPL-2.0
//! Declarative rows for the disposable development preview library.
//!
//! Add a row here when a new profile, launch-target or Review tools state needs
//! native UI coverage. The seeder in the parent module owns validation and uses
//! the ordinary profile, target, icon and fixture-store operations.
use carbon_profiles::VanillaVersion;

pub(super) struct ProfileRow {
    /// Stable documentation key and the key used by Home pin/recent rows.
    pub key: &'static str,
    pub name: &'static str,
    pub version: VanillaVersion,
    pub memory_mib: u32,
    pub window: (u32, u32),
    /// A built-in icon ID, or `None` for the automatic vanilla/pack/server icon.
    pub icon: Option<&'static str>,
    pub pack: Option<PackRow>,
    pub server: Option<&'static str>,
    pub worlds: &'static [&'static str],
    /// Worlds that are labeled and then removed to expose a missing-label row.
    pub missing_worlds: &'static [(&'static str, &'static str)],
    pub destinations: &'static [(&'static str, &'static str)],
    pub select: Selection,
}

pub(super) struct PackRow {
    pub loader: Option<&'static str>,
    pub pending_files: usize,
}

#[derive(Clone, Copy)]
pub(super) enum Selection {
    None,
    World(&'static str),
    Server(&'static str),
}

pub(super) struct HomeRows {
    pub pinned: &'static [&'static str],
    /// Most recently opened first. The seeder records these in reverse so the
    /// existing HomeOrder API produces this display order.
    pub recent: &'static [&'static str],
}

/// A varied first-run-like library: modern and legacy versions, a snapshot,
/// duplicate-like and long Unicode names, built-in and automatic icons, an
/// empty target list, a mixed world/server list, stale labels, a qualified and
/// a pending local pack, and two bound servers.
pub(super) const DEFAULT_PROFILES: &[ProfileRow] = &[
    ProfileRow {
        key: "vanilla-survival",
        name: "Vanilla survival",
        version: VanillaVersion::Release26_3,
        memory_mib: 2048,
        window: (1280, 720),
        icon: None,
        pack: None,
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "alex-a",
        name: "Alex",
        version: VanillaVersion::Release1_21_1,
        memory_mib: 3072,
        window: (1920, 1080),
        icon: Some("fox"),
        pack: None,
        server: None,
        worlds: &["Harbour"],
        missing_worlds: &[],
        destinations: &[("Lantern Bay", "play.example.org")],
        select: Selection::World("Harbour"),
    },
    ProfileRow {
        key: "alex-b",
        name: "Alex",
        version: VanillaVersion::Release1_20_4,
        memory_mib: 1024,
        window: (854, 480),
        icon: Some("frog"),
        pack: None,
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "unicode-snapshot",
        name: "苔むす森・夜間採集",
        version: VanillaVersion::Snapshot24w14a,
        memory_mib: 2048,
        window: (1280, 720),
        icon: Some("mooncap"),
        pack: None,
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "long-name",
        name: "Long name 0000000000 1111111111 2222222222 3333333333 4444444444 5555555555 6666",
        version: VanillaVersion::Release1_20_1,
        memory_mib: 4096,
        window: (1280, 720),
        icon: None,
        pack: None,
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "pack-qualified",
        name: "Pine Hills pack",
        version: VanillaVersion::Release1_21_1,
        memory_mib: 4096,
        window: (1280, 720),
        icon: Some("pine"),
        pack: Some(PackRow {
            loader: None,
            pending_files: 0,
        }),
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "pack-pending",
        name: "Fabric dev pack",
        version: VanillaVersion::Release1_21_1,
        memory_mib: 4096,
        window: (1280, 720),
        icon: Some("pack"),
        pack: Some(PackRow {
            loader: Some("fabric"),
            pending_files: 3,
        }),
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "bound-server",
        name: "Bound server · survival",
        version: VanillaVersion::Release26_3,
        memory_mib: 2048,
        window: (1280, 720),
        icon: Some("beacon"),
        pack: None,
        server: Some("play.example.org:25565"),
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "bound-ipv6",
        name: "Local IPv6 server",
        version: VanillaVersion::Release1_21_1,
        memory_mib: 2048,
        window: (1280, 720),
        icon: None,
        pack: None,
        server: Some("[::1]:25565"),
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "targets-empty",
        name: "Targets · none saved",
        version: VanillaVersion::Release26_3,
        memory_mib: 2048,
        window: (1280, 720),
        icon: Some("compass"),
        pack: None,
        server: None,
        worlds: &[],
        missing_worlds: &[],
        destinations: &[],
        select: Selection::None,
    },
    ProfileRow {
        key: "targets-mixed",
        name: "Targets · mixed",
        version: VanillaVersion::Release1_20_1,
        memory_mib: 3072,
        window: (1280, 720),
        icon: Some("crystal"),
        pack: None,
        server: None,
        worlds: &["Cove", "Highlands"],
        missing_worlds: &[],
        destinations: &[
            ("Lantern Bay", "play.example.org:25565"),
            ("Creative flat", "[::1]:25565"),
        ],
        select: Selection::Server("Lantern Bay"),
    },
    ProfileRow {
        key: "targets-stale-label",
        name: "Targets · stale label",
        version: VanillaVersion::Release1_20_4,
        memory_mib: 2048,
        window: (1280, 720),
        icon: Some("summit"),
        pack: None,
        server: None,
        worlds: &["Ridge"],
        missing_worlds: &[("Retired world", "Summer 2025")],
        destinations: &[],
        select: Selection::None,
    },
];

pub(super) const DEFAULT_HOME: HomeRows = HomeRows {
    pinned: &["targets-mixed", "bound-server"],
    recent: &["unicode-snapshot", "alex-a"],
};
