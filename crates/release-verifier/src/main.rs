use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Read,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Deserialize)]
struct UpdaterManifest {
    version: String,
    platforms: BTreeMap<String, UpdaterEntry>,
}

#[derive(Deserialize)]
struct UpdaterEntry {
    url: String,
    signature: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Aggregate updater verification failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let first = arguments.next().ok_or_else(usage)?;
    if first == "--check-key" {
        parse_public_key(
            &env::var("TAURI_UPDATER_PUBLIC_KEY")
                .map_err(|_| "TAURI_UPDATER_PUBLIC_KEY is unavailable".to_string())?,
        )?;
        println!("Updater public key is a valid Minisign key.");
        return Ok(());
    }
    let asset_root = PathBuf::from(first);
    let manifest_path = PathBuf::from(arguments.next().ok_or_else(usage)?);
    let required_platforms = arguments.collect::<BTreeSet<_>>();
    if required_platforms.is_empty() {
        return Err(usage());
    }
    let public_key = parse_public_key(
        &env::var("TAURI_UPDATER_PUBLIC_KEY")
            .map_err(|_| "TAURI_UPDATER_PUBLIC_KEY is unavailable".to_string())?,
    )?;
    let assets = collect_assets(&asset_root)?;
    let manifest_bytes = read_bounded(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest_signature_path = assets
        .get("latest.json.sig")
        .ok_or_else(|| "signed latest.json envelope is missing".to_string())?;
    let manifest_signature_text =
        String::from_utf8(read_bounded(manifest_signature_path, MAX_MANIFEST_BYTES)?)
            .map_err(|_| "latest.json signature is not UTF-8".to_string())?;
    let manifest_signature = Signature::decode(manifest_signature_text.trim())
        .map_err(|_| "latest.json signature is malformed".to_string())?;
    public_key
        .verify(&manifest_bytes[..], &manifest_signature, false)
        .map_err(|_| "latest.json signature did not verify".to_string())?;
    let manifest: UpdaterManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| "latest.json is malformed".to_string())?;
    if let Ok(expected) = env::var("RELEASE_TAG") {
        if manifest.version.trim_start_matches('v') != expected.trim_start_matches('v') {
            return Err("latest.json version does not match RELEASE_TAG".into());
        }
    }
    for platform in &required_platforms {
        if !manifest.platforms.contains_key(platform) {
            return Err(format!("latest.json is missing stable platform {platform}"));
        }
    }
    for (platform, entry) in &manifest.platforms {
        let url = url::Url::parse(&entry.url)
            .map_err(|_| format!("updater URL for {platform} is malformed"))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(format!(
                "updater URL for {platform} is not credential-free HTTPS"
            ));
        }
        let file_name = url
            .path_segments()
            .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
            .ok_or_else(|| format!("updater URL for {platform} has no artifact name"))?;
        let artifact = assets
            .get(file_name)
            .ok_or_else(|| format!("updater artifact for {platform} is not downloaded"))?;
        let signature_path = assets
            .get(&format!("{file_name}.sig"))
            .ok_or_else(|| format!("updater signature file for {platform} is missing"))?;
        let signature_file =
            String::from_utf8(read_bounded(signature_path, MAX_MANIFEST_BYTES)?)
                .map_err(|_| format!("updater signature file for {platform} is not UTF-8"))?;
        if signature_file.trim() != entry.signature.trim() {
            return Err(format!(
                "latest.json signature for {platform} differs from its uploaded .sig file"
            ));
        }
        let signature = Signature::decode(entry.signature.trim())
            .map_err(|_| format!("updater signature for {platform} is malformed"))?;
        verify_stream(&public_key, artifact, &signature)
            .map_err(|_| format!("updater signature for {platform} did not verify"))?;
    }
    let installer_suffixes = [".AppImage", ".deb", ".dmg", ".exe", ".msi"];
    for (name, artifact) in assets.iter().filter(|(name, _)| {
        installer_suffixes
            .iter()
            .any(|suffix| name.ends_with(suffix))
    }) {
        let signature_path = assets
            .get(&format!("{name}.sig"))
            .ok_or_else(|| format!("native installer signature missing for {name}"))?;
        let signature_text =
            String::from_utf8(read_bounded(signature_path, MAX_MANIFEST_BYTES)?)
                .map_err(|_| format!("native installer signature for {name} is not UTF-8"))?;
        let signature = Signature::decode(signature_text.trim())
            .map_err(|_| format!("native installer signature for {name} is malformed"))?;
        verify_stream(&public_key, artifact, &signature)
            .map_err(|_| format!("native installer signature for {name} did not verify"))?;
    }
    println!(
        "Verified {} updater signatures across {} required stable platforms.",
        manifest.platforms.len(),
        required_platforms.len()
    );
    Ok(())
}

fn usage() -> String {
    "usage: stm-release-verifier <asset-root> <latest.json> <required-platform>...".into()
}

fn parse_public_key(value: &str) -> Result<PublicKey, String> {
    if value.starts_with("RW") && !value.contains(['\r', '\n']) && value.trim() == value {
        return PublicKey::from_base64(value).map_err(|_| "updater public key is malformed".into());
    }
    let decoded = BASE64
        .decode(value.trim())
        .map_err(|_| "updater public key is neither Minisign text nor Tauri-encoded text")?;
    if decoded.len() > 2048 {
        return Err("decoded updater public key exceeds the accepted bound".into());
    }
    let document =
        std::str::from_utf8(&decoded).map_err(|_| "decoded updater public key is not UTF-8")?;
    let encoded = tauri_minisign_payload(document)
        .ok_or_else(|| "updater public key has no canonical Minisign document".to_string())?;
    PublicKey::from_base64(encoded).map_err(|_| "updater public key is malformed".into())
}

fn tauri_minisign_payload(value: &str) -> Option<&str> {
    let document = value.strip_suffix('\n').unwrap_or(value);
    let (comment, key) = document.split_once('\n')?;
    if key.contains('\n') {
        return None;
    }
    let id = comment.strip_prefix("untrusted comment: minisign public key: ")?;
    ((8..=16).contains(&id.len())
        && id.bytes().all(|byte| byte.is_ascii_hexdigit())
        && key.starts_with("RW"))
    .then_some(key)
}

fn collect_assets(root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let metadata =
        fs::symlink_metadata(root).map_err(|_| "release asset root is unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("release asset root must be a regular directory".into());
    }
    let mut assets = BTreeMap::new();
    collect_directory(root, root, 0, &mut assets)?;
    Ok(assets)
}

fn collect_directory(
    root: &Path,
    directory: &Path,
    depth: usize,
    assets: &mut BTreeMap<String, PathBuf>,
) -> Result<(), String> {
    if depth > 8 || assets.len() > 1000 {
        return Err("release asset tree exceeds bounds".into());
    }
    for entry in
        fs::read_dir(directory).map_err(|_| "release asset directory failed".to_string())?
    {
        let path = entry
            .map_err(|_| "release asset entry failed".to_string())?
            .path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|_| "release asset metadata failed".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("release asset symlink rejected".into());
        }
        if metadata.is_dir() {
            collect_directory(root, &path, depth + 1, assets)?;
            continue;
        }
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ARTIFACT_BYTES {
            return Err("release asset is empty, non-regular, or oversized".into());
        }
        if !path.starts_with(root) {
            return Err("release asset path escaped root".into());
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "release asset name is not UTF-8".to_string())?
            .to_string();
        if assets.insert(name.clone(), path).is_some() {
            return Err(format!("duplicate release asset name {name}"));
        }
    }
    Ok(())
}

fn read_bounded(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "bounded file unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > maximum {
        return Err("bounded file rejected".into());
    }
    let mut output = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .map_err(|_| "bounded file open failed".to_string())?
        .take(maximum + 1)
        .read_to_end(&mut output)
        .map_err(|_| "bounded file read failed".to_string())?;
    if output.len() as u64 > maximum {
        return Err("bounded file grew beyond limit".into());
    }
    Ok(output)
}

fn verify_stream(public_key: &PublicKey, path: &Path, signature: &Signature) -> Result<(), String> {
    let mut verifier = public_key
        .verify_stream(signature)
        .map_err(|_| "stream verifier rejected signature".to_string())?;
    let mut file = fs::File::open(path).map_err(|_| "updater artifact open failed".to_string())?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "updater artifact read failed".to_string())?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    verifier
        .finalize()
        .map_err(|_| "updater artifact signature mismatch".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minisign_known_answer_accepts_exact_bytes_and_rejects_tampering() {
        let public_key =
            PublicKey::from_base64("RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3")
                .expect("public key");
        let signature = Signature::decode(
            "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1633700835\tfile:test\tprehashed\nwLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==",
        )
        .expect("signature");
        public_key
            .verify(&b"test"[..], &signature, false)
            .expect("known answer");
        assert!(public_key
            .verify(&b"tampered"[..], &signature, false)
            .is_err());
    }

    #[test]
    fn accepts_tauri_encoded_minisign_public_key() {
        let document = "untrusted comment: minisign public key: 0123456789ABCD\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n";
        let encoded = BASE64.encode(document);
        parse_public_key(&encoded).expect("Tauri-encoded public key");
    }

    #[test]
    fn rejects_tauri_wrapper_with_trailing_secret_material() {
        let document = "untrusted comment: minisign public key: 0123456789ABCDEF\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\nuntrusted comment: minisign secret key\nRWRleGFtcGxl\n";
        let encoded = BASE64.encode(document);
        assert!(parse_public_key(&encoded).is_err());
    }

    #[test]
    fn rejects_noncanonical_tauri_wrappers() {
        let key = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        assert!(parse_public_key(&BASE64.encode(format!("{key}\n"))).is_err());
        let padded = format!("untrusted comment: minisign public key: 0123456789ABCD\n {key}\n");
        assert!(parse_public_key(&BASE64.encode(padded)).is_err());
    }
}
