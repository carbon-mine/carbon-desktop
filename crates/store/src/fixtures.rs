#[derive(Clone, Copy)]
pub(crate) struct Blob {
    pub name: &'static str,
    pub digest: &'static str,
    pub kib: usize,
    pub(crate) seed: u32,
}

impl Blob {
    // Fixture v1: deterministic inert bytes. The checked-in digest is the expected identity.
    pub fn source(&self) -> Vec<u8> {
        let mut state = self.seed;
        (0..self.kib * 1024)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect()
    }

    pub fn len(&self) -> u64 {
        self.kib as u64 * 1024
    }
}

pub(crate) const ALL: [Blob; 6] = [
    Blob {
        name: "Terrain",
        digest: "f450763f99b2030b786d79d7d3aa496cd2fd61b94dd5bf1212fcddfdd363045e",
        kib: 48,
        seed: 1,
    },
    Blob {
        name: "Lighting",
        digest: "c4f2334fe0f8ea4b952d1832b6a349da69fb248d46e5912f84d35e5bc005c963",
        kib: 12,
        seed: 2,
    },
    Blob {
        name: "Forest",
        digest: "09720ef743705423277720d41837be950488ff6dd6f4baf36609270f54198062",
        kib: 16,
        seed: 3,
    },
    Blob {
        name: "Coast",
        digest: "97b58cae8501d8276c50c9fbeecddd9500887740b5803e9f3b71af37fb16ae29",
        kib: 16,
        seed: 4,
    },
    Blob {
        name: "Rain A",
        digest: "d9323dac5a6ba54f43c2add7db936b4620af644bfa083bc113029dec46b51c92",
        kib: 4,
        seed: 5,
    },
    Blob {
        name: "Rain B",
        digest: "31a884f803ae82179f9fe1f7d30ecff571426557e3e866ee4aab49f6c957b130",
        kib: 4,
        seed: 6,
    },
];
