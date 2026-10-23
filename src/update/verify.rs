//! Download checks: the release's `SHA256SUMS.txt` must carry a valid
//! Ed25519 signature from a key in `packaging/update-keys.txt`, and the
//! package's SHA-256 must match its line there.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

const KEYS: &str = include_str!("../../packaging/update-keys.txt");

/// The release signing keys built into the game.
pub fn release_keys() -> Vec<[u8; 32]> {
    KEYS.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| unhex(l).and_then(|b| b.try_into().ok()).expect("packaging/update-keys.txt holds 32-byte hex keys"))
        .collect()
}

/// Checks `signature` (hex, as written by the release workflow) over the
/// exact bytes of the checksum file.
pub fn check_signature(sums: &[u8], signature: &str, keys: &[[u8; 32]]) -> Result<(), String> {
    let bad = || "the release's checksums are not signed by VoxelCraft's release key".to_string();
    let bytes = unhex(signature.trim()).ok_or_else(bad)?;
    let signature = ed25519_compact::Signature::from_slice(&bytes).map_err(|_| bad())?;
    keys.iter()
        .filter_map(|k| ed25519_compact::PublicKey::from_slice(k).ok())
        .any(|k| k.verify(sums, &signature).is_ok())
        .then_some(())
        .ok_or_else(bad)
}

/// The hash `sums` (in `sha256sum` format) lists for `name`.
pub fn listed_hash(sums: &str, name: &str) -> Option<[u8; 32]> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(' ')?;
        // Text mode has two spaces before the name, binary mode " *".
        let file = file.strip_prefix(' ').or_else(|| file.strip_prefix('*')).unwrap_or(file);
        (file == name).then(|| unhex(hash)?.try_into().ok()).flatten()
    })
}

pub fn sha256_file(path: &Path) -> std::io::Result<[u8; 32]> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        match file.read(&mut buf)? {
            0 => break,
            n => hasher.update(&buf[..n]),
        }
    }
    Ok(hasher.finalize().into())
}

/// Checks the downloaded package against the checksum list signed by one
/// of `keys`.
pub fn check_package(
    package: &Path,
    name: &str,
    sums: &[u8],
    signature: &str,
    keys: &[[u8; 32]],
) -> Result<(), String> {
    check_signature(sums, signature, keys)?;
    let sums = std::str::from_utf8(sums).map_err(|_| "the checksum list is not text".to_string())?;
    let expected = listed_hash(sums, name).ok_or_else(|| format!("the checksum list doesn't include {name}"))?;
    let actual = sha256_file(package).map_err(|e| format!("could not read the download: {e}"))?;
    if actual != expected {
        return Err("the download is damaged: its checksum doesn't match the release".into());
    }
    Ok(())
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Made with `openssl genpkey -algorithm ed25519` and `openssl pkeyutl
    /// -sign -rawin`, as the release workflow signs (a throwaway key).
    const OPENSSL_KEY: &str = "0577e78ef512f5512f49d4d99a77105a01faca2a9815ee7c74436d880680a9c1";
    const OPENSSL_SUMS: &str = "abc123  VoxelCraft-0.11.0-windows-x64-Setup.exe\n";
    const OPENSSL_SIG: &str = "adc27bef2c8e1b7e31a6b8a46f2f57f5183345f2860a3d6b69b66f5ab8d20d67\
                               61aaf3b52f3965a6f075d49aeab6fb6495fdbd45fe403877a5fb4b4831a5c50a";

    fn key(hex: &str) -> [u8; 32] {
        unhex(hex).unwrap().try_into().unwrap()
    }

    #[test]
    fn accepts_openssl_signatures() {
        let keys = [key(OPENSSL_KEY)];
        assert_eq!(check_signature(OPENSSL_SUMS.as_bytes(), OPENSSL_SIG, &keys), Ok(()));
        assert_eq!(check_signature(OPENSSL_SUMS.as_bytes(), &format!("{OPENSSL_SIG}\n"), &keys), Ok(()));
        // Any change to the list, a wrong key or a malformed signature fails.
        let tampered = OPENSSL_SUMS.replace("abc123", "abc124");
        assert!(check_signature(tampered.as_bytes(), OPENSSL_SIG, &keys).is_err());
        assert!(check_signature(OPENSSL_SUMS.as_bytes(), OPENSSL_SIG, &release_keys()).is_err());
        assert!(check_signature(OPENSSL_SUMS.as_bytes(), "zz", &keys).is_err());
        assert!(check_signature(OPENSSL_SUMS.as_bytes(), "", &keys).is_err());
        // The second of two keys can sign (key rotation).
        assert_eq!(check_signature(OPENSSL_SUMS.as_bytes(), OPENSSL_SIG, &[release_keys()[0], keys[0]]), Ok(()));
    }

    #[test]
    fn release_key_is_built_in() {
        assert!(!release_keys().is_empty());
    }

    #[test]
    fn finds_listed_hashes() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let sums =
            format!("{a}  VoxelCraft-0.11.0-macos-apple-silicon.dmg\n{b} *VoxelCraft-0.11.0-windows-x64-Setup.exe\n");
        assert_eq!(listed_hash(&sums, "VoxelCraft-0.11.0-macos-apple-silicon.dmg"), Some([0xaa; 32]));
        assert_eq!(listed_hash(&sums, "VoxelCraft-0.11.0-windows-x64-Setup.exe"), Some([0xbb; 32]));
        assert_eq!(listed_hash(&sums, "VoxelCraft-0.11.0"), None);
        assert_eq!(listed_hash("zz  name\n", "name"), None);
    }

    #[test]
    fn hashes_files() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-sha-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("abc");
        std::fs::write(&file, "abc").unwrap();
        assert_eq!(
            hex(&sha256_file(&file).unwrap()),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
