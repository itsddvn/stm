use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;

use crate::error::CoreError;

#[derive(Debug, Clone)]
pub struct FixtureWorkspace {
    project_root: PathBuf,
    db_path_override: Option<PathBuf>,
}

impl FixtureWorkspace {
    pub fn new(project_root: impl Into<PathBuf>) -> Self {
        Self {
            project_root: project_root.into(),
            db_path_override: None,
        }
    }
    pub fn with_db_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.db_path_override = Some(path.into());
        self
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn fixture_root(&self) -> PathBuf {
        self.project_root.join("tests/fixtures")
    }

    pub fn catalog_root(&self) -> PathBuf {
        self.project_root.join("catalog")
    }

    pub fn db_path(&self) -> PathBuf {
        self.db_path_override
            .clone()
            .unwrap_or_else(|| self.project_root.join("target/stm-phase-three/stm.sqlite"))
    }

    pub fn read_json<T>(&self, relative: &str) -> Result<T, CoreError>
    where
        T: DeserializeOwned,
    {
        let raw = self.read_text(relative)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn read_json_if_exists<T>(&self, relative: &str) -> Result<Option<T>, CoreError>
    where
        T: DeserializeOwned,
    {
        let path = self.resolve(relative);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&fs::read_to_string(path)?)?))
    }

    pub fn read_json_value(&self, relative: &str) -> Result<JsonValue, CoreError> {
        let raw = self.read_text(relative)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn read_text(&self, relative: &str) -> Result<String, CoreError> {
        Ok(fs::read_to_string(self.resolve(relative))?)
    }

    pub fn resolve(&self, relative: &str) -> PathBuf {
        self.project_root.join(relative)
    }
}

pub fn ensure_https_url(value: &str, field: &str) -> Result<(), CoreError> {
    if value.starts_with("https://") {
        Ok(())
    } else {
        Err(CoreError::MalformedInput(format!(
            "{field} must be https, found: {value}"
        )))
    }
}

pub fn compute_sha256(parts: impl IntoIterator<Item = Vec<u8>>) -> String {
    let mut state = Sha256State::new();
    for part in parts {
        state.update(&part);
    }
    format!("sha256:{}", state.finish_hex())
}

pub fn json_object<'a>(
    value: &'a JsonValue,
    context: &str,
) -> Result<&'a serde_json::Map<String, JsonValue>, CoreError> {
    value
        .as_object()
        .ok_or_else(|| CoreError::MalformedInput(format!("{context} must be a JSON object")))
}

pub fn json_array<'a>(
    value: &'a JsonValue,
    context: &str,
) -> Result<&'a Vec<JsonValue>, CoreError> {
    value
        .as_array()
        .ok_or_else(|| CoreError::MalformedInput(format!("{context} must be a JSON array")))
}

pub fn json_string(value: &JsonValue, context: &str) -> Result<String, CoreError> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| CoreError::MalformedInput(format!("{context} must be a string")))
}

pub fn sort_map_by_key<K, V>(source: BTreeMap<K, V>) -> Vec<V>
where
    K: Ord,
{
    source.into_values().collect()
}

pub(crate) struct Sha256State {
    buffer: [u8; 64],
    buffer_len: usize,
    bit_len: u64,
    state: [u32; 8],
}

impl Sha256State {
    pub(crate) fn new() -> Self {
        Self {
            buffer: [0; 64],
            buffer_len: 0,
            bit_len: 0,
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
        }
    }

    pub(crate) fn update(&mut self, data: &[u8]) {
        for byte in data {
            self.buffer[self.buffer_len] = *byte;
            self.buffer_len += 1;
            if self.buffer_len == 64 {
                self.transform();
                self.bit_len = self.bit_len.wrapping_add(512);
                self.buffer_len = 0;
            }
        }
    }

    pub(crate) fn finish_hex(mut self) -> String {
        let bit_len = self.bit_len.wrapping_add((self.buffer_len as u64) * 8);
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;

        if self.buffer_len > 56 {
            while self.buffer_len < 64 {
                self.buffer[self.buffer_len] = 0;
                self.buffer_len += 1;
            }
            self.transform();
            self.buffer_len = 0;
        }

        while self.buffer_len < 56 {
            self.buffer[self.buffer_len] = 0;
            self.buffer_len += 1;
        }

        self.buffer[56..64].copy_from_slice(&bit_len.to_be_bytes());
        self.transform();

        let mut output = String::with_capacity(64);
        for value in self.state {
            output.push_str(&format!("{value:08x}"));
        }
        output
    }

    fn transform(&mut self) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let mut words = [0_u32; 64];
        for (index, chunk) in self.buffer.chunks_exact(4).take(16).enumerate() {
            words[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

#[cfg(test)]
mod tests {
    use super::compute_sha256;

    #[test]
    fn computes_stable_sha256() {
        let digest = compute_sha256([b"abc".to_vec()]);
        assert_eq!(
            digest,
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
