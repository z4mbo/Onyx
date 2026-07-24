use std::{
    env,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    process::ExitCode,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use serde_json::Value;

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 64 * 1024;
const VERIFY_BUFFER_BYTES: usize = 64 * 1024;

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(archive) => {
            println!(
                "Verified updater signature for {}",
                archive.to_string_lossy()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("updater signature verification failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: Vec<std::ffi::OsString>) -> Result<PathBuf, String> {
    if arguments.len() != 3 {
        return Err(
            "usage: verify-updater-signature <tauri.conf.json> <archive> <archive.sig>".to_owned(),
        );
    }

    let config_path = PathBuf::from(&arguments[0]);
    let archive_path = PathBuf::from(&arguments[1]);
    let signature_path = PathBuf::from(&arguments[2]);

    ensure_regular_file(&config_path)?;
    ensure_regular_file(&archive_path)?;
    ensure_regular_file(&signature_path)?;

    let config = read_bounded(&config_path, MAX_CONFIG_BYTES)?;
    let config: Value = serde_json::from_slice(&config)
        .map_err(|error| format!("could not parse {}: {error}", config_path.display()))?;
    let encoded_public_key = config
        .pointer("/plugins/updater/pubkey")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "{} has no non-empty plugins.updater.pubkey",
                config_path.display()
            )
        })?;
    let public_key = decode_public_key(encoded_public_key)?;

    let encoded_signature = read_bounded(&signature_path, MAX_SIGNATURE_BYTES)?;
    let signature = decode_signature(&encoded_signature)?;
    let mut verifier = public_key
        .verify_stream(&signature)
        .map_err(|error| format!("could not initialize streaming verification: {error}"))?;

    let mut archive = File::open(&archive_path)
        .map_err(|error| format!("could not open {}: {error}", archive_path.display()))?;
    let mut buffer = [0_u8; VERIFY_BUFFER_BYTES];
    loop {
        let read = archive
            .read(&mut buffer)
            .map_err(|error| format!("could not read {}: {error}", archive_path.display()))?;
        if read == 0 {
            break;
        }
        verifier.update(&buffer[..read]);
    }
    verifier
        .finalize()
        .map_err(|error| format!("signature is not valid for the archive: {error}"))?;

    Ok(archive_path)
}

fn ensure_regular_file(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "{} must be a non-empty regular file, not a symlink",
            path.display()
        ));
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("could not inspect {}: {error}", path.display()))?;
    if metadata.len() > limit {
        return Err(format!(
            "{} exceeds the {} byte safety limit",
            path.display(),
            limit
        ));
    }
    fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))
}

fn decode_public_key(encoded: &str) -> Result<PublicKey, String> {
    let decoded = decode_base64_text(encoded.as_bytes(), "updater public key")?;
    PublicKey::decode(&decoded)
        .map_err(|error| format!("the configured updater public key is invalid: {error}"))
}

fn decode_signature(encoded: &[u8]) -> Result<Signature, String> {
    let decoded = decode_base64_text(encoded, "updater signature")?;
    Signature::decode(&decoded)
        .map_err(|error| format!("the updater signature envelope is invalid: {error}"))
}

fn decode_base64_text(encoded: &[u8], label: &str) -> Result<String, String> {
    let compact: Vec<u8> = encoded
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if compact.is_empty() {
        return Err(format!("{label} is empty"));
    }
    let decoded = STANDARD
        .decode(compact)
        .map_err(|error| format!("{label} is not valid base64: {error}"))?;
    String::from_utf8(decoded).map_err(|_| format!("{label} does not decode to UTF-8 text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_updater_public_key_decodes() {
        let config: Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("valid Tauri config");
        let encoded = config
            .pointer("/plugins/updater/pubkey")
            .and_then(Value::as_str)
            .expect("updater public key");

        decode_public_key(encoded).expect("checked-in updater key should be a Minisign key");
    }

    #[test]
    fn whitespace_is_allowed_around_base64_envelopes() {
        let encoded = b" dGVz\n dA==\t";
        assert_eq!(decode_base64_text(encoded, "fixture").unwrap(), "test");
    }

    #[test]
    fn tauri_style_envelopes_verify_in_streaming_mode() {
        let public_key_text = concat!(
            "untrusted comment: minisign public key\n",
            "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3\n"
        );
        let signature_text = concat!(
            "untrusted comment: signature from minisign secret key\n",
            "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/",
            "z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n",
            "trusted comment: timestamp:1556193335\tfile:test\n",
            "y/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR",
            "1FkZZSNCisQbuQY+bHwhEBg=="
        );
        let public_key = decode_public_key(&STANDARD.encode(public_key_text)).unwrap();
        let signature = decode_signature(STANDARD.encode(signature_text).as_bytes()).unwrap();
        let mut verifier = public_key.verify_stream(&signature).unwrap();
        verifier.update(b"te");
        verifier.update(b"st");

        verifier.finalize().unwrap();
    }

    #[test]
    fn oversized_reads_are_rejected_before_allocation() {
        let path = env::temp_dir().join(format!(
            "onyx-updater-signature-test-{}",
            std::process::id()
        ));
        fs::write(&path, [0_u8; 8]).unwrap();
        let result = read_bounded(&path, 4);
        let _ = fs::remove_file(path);

        assert!(result.unwrap_err().contains("safety limit"));
    }
}
