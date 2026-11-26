// SPDX-License-Identifier: MPL-2.0
// Synthetic identities for the native design review.
#[path = "bust.rs"]
mod bust;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Profile {
    pub name: &'static str,
    pub id: &'static str,
    pub skin: Option<&'static str>,
    pub slim: bool,
}

pub const PROFILES: [Profile; 3] = [
    Profile {
        name: "AlderStone",
        id: "00000000-0000-4000-a000-000000000001",
        skin: Some("alder"),
        slim: false,
    },
    Profile {
        name: "EmberVale",
        id: "00000000-0000-4000-a000-000000000002",
        skin: Some("ember"),
        slim: true,
    },
    Profile {
        name: "SiltVale",
        id: "00000000-0000-4000-a000-000000000003",
        skin: None,
        slim: false,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupError {
    Invalid,
    Missing,
}

fn uuid_digits(input: &str) -> Option<String> {
    let valid = match input.len() {
        32 => input.bytes().all(|c| c.is_ascii_hexdigit()),
        36 => input.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        }),
        _ => false,
    };
    valid.then(|| input.replace('-', "").to_ascii_lowercase())
}

pub fn lookup(input: &str) -> Result<Profile, LookupError> {
    let input = input.trim();
    if let Some(id) = uuid_digits(input) {
        return PROFILES
            .iter()
            .copied()
            .find(|p| p.id.replace('-', "") == id)
            .ok_or(LookupError::Missing);
    }
    // Some legacy identities have short names. Creation rules need not constrain an exact lookup.
    if input.is_empty()
        || input.len() > 16
        || !input
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(LookupError::Invalid);
    }
    PROFILES
        .iter()
        .copied()
        .find(|p| p.name.eq_ignore_ascii_case(input))
        .ok_or(LookupError::Missing)
}

pub fn bust_rgba(skin: &str, yaw: f32, pitch: f32, side: usize) -> Vec<u8> {
    let (pixels, slim) = match skin {
        "alder" => (include_bytes!("alder.rgba"), false),
        "ember" => (include_bytes!("ember.rgba"), true),
        _ => return Vec::new(),
    };
    if !(1..=512).contains(&side) || !yaw.is_finite() || !pitch.is_finite() {
        return Vec::new();
    }
    bust::render(
        pixels.as_slice(),
        slim,
        yaw % 360.0,
        pitch.clamp(-60.0, 60.0),
        side,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_name_and_uuid_aliases_match_each_identity() {
        for p in PROFILES {
            for input in [
                p.name.to_lowercase(),
                p.id.to_uppercase(),
                p.id.replace('-', ""),
            ] {
                assert_eq!(lookup(&input), Ok(p));
            }
        }
    }
    #[test]
    fn distinguishes_unknown_from_invalid() {
        assert_eq!(lookup("NoSuchFixture"), Err(LookupError::Missing));
        for bad in [
            "",
            "../AlderStone",
            "000000000-000-4000-a000-000000000001",
            "Alder Stone",
        ] {
            assert_eq!(lookup(bad), Err(LookupError::Invalid));
        }
    }
}
