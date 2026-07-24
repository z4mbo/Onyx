#[cfg(target_os = "macos")]
pub async fn exists(service: &str, account: &str) -> Result<bool, String> {
    let service = service.to_owned();
    let account = account.to_owned();
    tokio::task::spawn_blocking(move || {
        std::process::Command::new("/usr/bin/security")
            .args([
                "find-generic-password",
                "-s",
                service.as_str(),
                "-a",
                account.as_str(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(not(target_os = "macos"))]
pub async fn exists(service: &str, account: &str) -> Result<bool, String> {
    let service = service.to_owned();
    let account = account.to_owned();
    tokio::task::spawn_blocking(move || {
        let entry = keyring::Entry::new(&service, &account).map_err(|error| error.to_string())?;
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    })
    .await
    .map_err(|error| error.to_string())?
}
