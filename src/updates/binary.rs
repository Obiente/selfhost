//! Official standalone release downloads and replacement with an executable backup.
use super::*;
pub(super) fn asset() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("selfhost-linux-x64"),
        ("linux", "aarch64") => Ok("selfhost-linux-arm64"),
        ("macos", "x86_64") => Ok("selfhost-darwin-x64"),
        ("macos", "aarch64") => Ok("selfhost-darwin-arm64"),
        ("windows", "x86_64") => Ok("selfhost-win32-x64.exe"),
        ("windows", "aarch64") => Ok("selfhost-win32-arm64.exe"),
        _ => bail!("No standalone Selfhost release is available for this platform"),
    }
}
fn checksum(text: &str, name: &str) -> Result<String> {
    let entries = text
        .lines()
        .filter_map(|l| {
            let p: Vec<_> = l.split_whitespace().collect();
            (p.len() == 2 && p[1] == name).then(|| p[0])
        })
        .collect::<Vec<_>>();
    ensure!(
        entries.len() == 1
            && entries[0].len() == 64
            && entries[0].bytes().all(|b| b.is_ascii_hexdigit()),
        "Release manifest has no unique valid checksum for this platform"
    );
    Ok(entries[0].to_ascii_lowercase())
}
async fn download(version: &str, name: &str, limit: usize) -> Result<Vec<u8>> {
    stable_version(version)?;
    ensure!(
        name == "BINARY-SHA256SUMS" || name == asset()?,
        "Unsupported release asset"
    );
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(concat!("selfhost/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut response = client
        .get(format!(
            "https://github.com/Obiente/selfhost/releases/download/v{version}/{name}"
        ))
        .send()
        .await?
        .error_for_status()?;
    ensure!(
        response.content_length().unwrap_or(0) <= limit as u64,
        "Release asset exceeds its size limit"
    );
    let mut data = vec![];
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            data.len() + chunk.len() <= limit,
            "Release asset exceeds its size limit"
        );
        data.extend_from_slice(&chunk);
    }
    Ok(data)
}
pub(super) async fn release_hash(version: &str) -> Result<String> {
    let bytes = download(version, "BINARY-SHA256SUMS", 64 * 1024).await?;
    checksum(std::str::from_utf8(&bytes)?, asset()?)
}
pub(super) fn stage(path: &Path, job: &mut Job) -> Result<()> {
    let expected = job
        .plan
        .release_hash
        .as_deref()
        .context("Missing approved binary checksum; review a new update plan")?;
    let bytes = tokio::runtime::Runtime::new()?.block_on(download(
        &job.version,
        asset()?,
        128 * 1024 * 1024,
    ))?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected,
        "Downloaded binary does not match the approved release checksum"
    );
    let bin = path.join("stage/bin");
    private_dir(&bin)?;
    let executable = bin.join(EXE);
    atomic_write(&executable, &bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
    }
    native(&executable)?;
    let mut command = tokio::process::Command::new(&executable);
    command.arg("--version");
    let version = tokio::runtime::Runtime::new()?.block_on(crate::core::run(command, 10))?;
    ensure!(
        version.trim() == format!("selfhost {}", job.version),
        "Staged binary reports an unexpected version"
    );
    job.staged_hash = Some(expected.into());
    job.status = "staged".into();
    Ok(())
}
fn check_file(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Standalone update target must be an ordinary executable file"
    );
    Ok(())
}
pub(super) fn replace(path: &Path, job: &mut Job) -> Result<()> {
    let install = &job.plan.installation;
    ensure!(
        install.method == "standalone",
        "Unexpected installation method"
    );
    let parent = install
        .executable
        .parent()
        .context("Missing executable directory")?;
    let _lock = lock(&parent.join(".selfhost-update.lock"))?;
    check_file(&install.executable)?;
    ensure!(
        hash(&install.executable)? == install.hash,
        "Installed executable changed after review"
    );
    let staged = path.join("stage/bin").join(EXE);
    check_file(&staged)?;
    let expected = job
        .staged_hash
        .as_deref()
        .context("Missing staged checksum")?;
    ensure!(
        job.plan.release_hash.as_deref() == Some(expected) && hash(&staged)? == expected,
        "Staged executable changed after review"
    );
    native(&staged)?;
    let backup = install
        .executable
        .with_file_name(format!(".selfhost-{}.backup", job.job_id));
    let replacement = install
        .executable
        .with_file_name(format!(".selfhost-{}.replacement", job.job_id));
    ensure!(
        !backup.exists() && !replacement.exists(),
        "Recovery files already exist; inspect the previous update"
    );
    // create_new prevents overwriting an independently-created sibling file.
    let mut destination = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&replacement)?;
    std::io::copy(&mut File::open(&staged)?, &mut destination)?;
    destination.sync_all()?;
    drop(destination);
    fs::set_permissions(
        &replacement,
        fs::metadata(&install.executable)?.permissions(),
    )?;
    ensure!(
        hash(&replacement)? == expected,
        "Replacement copy failed checksum verification"
    );
    job.status = "replacing".into();
    save(&path.join("job.json"), job)?;
    replace_with_backup(&job.plan.installation.executable, &replacement, &backup)?;
    if hash(&job.plan.installation.executable)? != job.staged_hash.as_deref().unwrap() {
        job.status = "rollback_required".into();
        bail!("Replacement verification failed; keep the original sibling backup for recovery");
    }
    job.status = "completed".into();
    save(&path.join("job.json"), job)?;
    Ok(())
}
pub(super) fn recover(path: &Path, job: &mut Job) -> Result<()> {
    let install = &job.plan.installation;
    let parent = install
        .executable
        .parent()
        .context("Missing executable directory")?;
    let _lock = lock(&parent.join(".selfhost-update.lock"))?;
    let target = &install.executable;
    let backup = target.with_file_name(format!(".selfhost-{}.backup", job.job_id));
    if target.exists() {
        check_file(target)?;
        if hash(target)? == install.hash {
            job.status = "recovered".into();
            job.error = None;
            save(&path.join("job.json"), job)?;
            return Ok(());
        }
    }
    check_file(&backup)?;
    ensure!(
        hash(&backup)? == install.hash,
        "Original executable backup changed"
    );
    if target.exists() {
        ensure!(
            Some(hash(target)?) == job.staged_hash,
            "Executable changed independently; refusing recovery"
        );
        let rejected = target.with_file_name(format!(".selfhost-{}.rejected", job.job_id));
        ensure!(!rejected.exists(), "A previous recovery file exists");
        replace_with_backup(target, &backup, &rejected)?;
    } else {
        fs::rename(&backup, target)?;
    }
    ensure!(
        hash(target)? == install.hash,
        "Recovered executable verification failed"
    );
    job.status = "recovered".into();
    job.error = None;
    save(&path.join("job.json"), job)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksums_reject_missing_duplicate_and_malformed_assets() {
        let name = "selfhost-linux-x64";
        let digest = "a".repeat(64);
        assert_eq!(
            checksum(&format!("{digest}  {name}\n"), name).unwrap(),
            digest
        );
        for text in [
            format!("{digest} other"),
            format!("bad {name}"),
            format!("{digest} {name}\n{digest} {name}"),
        ] {
            assert!(checksum(&text, name).is_err());
        }
    }
}
