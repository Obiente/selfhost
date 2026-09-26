//! Explicit image selection and read-only upstream discovery. Never changes running containers.
use crate::{catalog::AppInfo, core::Store};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestedVersion {
    pub image: String,
    pub label: String,
    pub notes: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub repository: String,
    #[serde(default)]
    pub tag_prefix: String,
    #[serde(default)]
    pub image_prefix: String,
    /// Only patch releases within this explicit major.minor stream may be proposed.
    pub stream: String,
    #[serde(default)]
    pub automatic: bool,
    pub smoke_path: String,
    pub smoke_status: u16,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema: u32,
    pub tested: Vec<TestedVersion>,
    pub source: Option<Source>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub image: String,
    #[serde(default)]
    pub allow_untested: bool,
}
pub fn image_repository(image: &str) -> Result<&str> {
    ensure!(
        !image.is_empty()
            && image.len() <= 512
            && image
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-:@".contains(&b)),
        "Use an explicit Docker image tag or sha256 digest"
    );
    if let Some((repository, digest)) = image.split_once('@') {
        ensure!(
            !repository.is_empty()
                && digest
                    .strip_prefix("sha256:")
                    .is_some_and(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())),
            "Use a complete sha256 image digest"
        );
        return Ok(repository);
    }
    let (repository, tag) = image
        .rsplit_once(':')
        .context("An explicit image tag is required")?;
    ensure!(
        !repository.is_empty() && !tag.is_empty() && !tag.contains('/') && tag.len() <= 128,
        "Invalid image tag"
    );
    Ok(repository)
}
fn stable(text: &str) -> Option<[u64; 3]> {
    let parts: Vec<_> = text.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut out = [0; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty()
            || p.len() > 10
            || (p.len() > 1 && p.starts_with('0'))
            || !p.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        out[i] = p.parse().ok()?;
    }
    Some(out)
}
impl Profile {
    pub fn validate(&self, default_image: &str) -> Result<()> {
        ensure!(
            self.schema == 1 && self.tested.len() <= 64,
            "Invalid version profile"
        );
        let repository = image_repository(default_image)?;
        let mut seen = std::collections::HashSet::new();
        for item in &self.tested {
            ensure!(
                image_repository(&item.image)? == repository
                    && seen.insert(&item.image)
                    && !item.label.trim().is_empty()
                    && !item.notes.trim().is_empty(),
                "Tested versions require unique images from the recipe repository and validation notes"
            );
        }
        if let Some(source) = &self.source {
            let parts: Vec<_> = source.repository.split('/').collect();
            ensure!(
                parts.len() == 2
                    && parts.iter().all(|p| !p.is_empty()
                        && p.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))),
                "Use an upstream GitHub owner/repository"
            );
            ensure!(
                stable(&format!("{}.0", source.stream)).is_some(),
                "Use an explicit major.minor update stream"
            );
            ensure!(
                [&source.tag_prefix, &source.image_prefix]
                    .iter()
                    .all(|p| p.len() <= 32
                        && p.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))),
                "Invalid version prefix"
            );
            ensure!(
                source.smoke_path.starts_with('/')
                    && !source.smoke_path.starts_with("//")
                    && !source.smoke_path.contains(['\\', '\r', '\n', '#'])
                    && (200..=399).contains(&source.smoke_status),
                "Invalid smoke probe"
            );
        }
        Ok(())
    }
}
pub fn options(app: &AppInfo) -> Value {
    let mut choices: Vec<Value> = app
        .versions
        .as_ref()
        .map(|p| {
            p.tested
                .iter()
                .map(|v| json!({"image":v.image,"label":v.label,"notes":v.notes,"tested":true}))
                .collect()
        })
        .unwrap_or_default();
    if !choices.iter().any(|c| c["image"] == app.image) {
        choices.insert(0,json!({"image":app.image,"label":"Recipe default","notes":"No separate version validation record is available.","tested":false}));
    }
    json!({"app":app.id,"default_image":app.image,"choices":choices,"custom_allowed":true,"custom_requires_acknowledgement":true})
}
pub fn select(app: &AppInfo, selection: &Selection) -> Result<Value> {
    ensure!(
        image_repository(&selection.image)? == image_repository(&app.image)?,
        "Select a tag or digest from this app's image repository; use a custom setup for a different image"
    );
    let tested = app
        .versions
        .as_ref()
        .is_some_and(|p| p.tested.iter().any(|v| v.image == selection.image));
    ensure!(
        tested || selection.image == app.image || selection.allow_untested,
        "This image is not a tested recipe version. Review its configuration and migrations, then explicitly allow an untested image"
    );
    let mut warnings=vec!["Changing an image may require application or database migrations. Back up application data first. Saving this choice does not pull, restart or downgrade running containers.".to_owned()];
    if !tested {
        warnings.push("Compatibility of this image with the recipe, native settings and setup workflows has not been verified.".into());
    }
    Ok(json!({"image":selection.image,"tested":tested,"warnings":warnings}))
}
impl Store {
    fn version_revision(&self, id: &str) -> Result<String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &self.project(id)?,
                &self.setup(id)?,
                &self.server(&self.project(id)?.server_id)?,
                self.standalone.as_ref().map(|d| d.revision()).transpose()?
            ))?)
        ))
    }
    pub fn version_plan(&self, id: &str, service: &str, selection: &Selection) -> Result<Value> {
        let project = self.project(id)?;
        self.project_docker(&project, true)?;
        let app = project
            .services
            .iter()
            .find(|s| s.app == service)
            .context("Service not found")?;
        let selected = select(&app.definition, selection)?;
        let from = self.setup(id)?.compose["services"][service]["image"]
            .as_str()
            .context("Service image missing")?
            .to_owned();
        let revision = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                self.version_revision(id)?,
                service,
                selection
            ))?)
        );
        Ok(
            json!({"revision":revision,"service":service,"from":from,"to":selection.image,"tested":selected["tested"],"warnings":selected["warnings"],"restart_required":from!=selection.image}),
        )
    }
    pub fn version_apply(
        &self,
        id: &str,
        service: &str,
        selection: &Selection,
        revision: &str,
    ) -> Result<Value> {
        let _lock = self.project_lock(id)?;
        let plan = self.version_plan(id, service, selection)?;
        ensure!(
            plan["revision"].as_str() == Some(revision),
            "Configuration changed; review a fresh version plan"
        );
        if plan["from"] != plan["to"] {
            let mut setup = self.setup(id)?;
            setup.compose["services"][service]["image"] = json!(selection.image);
            self.save_setup_locked(id, setup)?;
        }
        Ok(
            json!({"saved":true,"image":selection.image,"restart_required":plan["restart_required"],"running_services_changed":false}),
        )
    }
    pub async fn catalog_version_check(&self, app: &str) -> Result<Value> {
        let recipe = self
            .catalog
            .iter()
            .find(|a| a.id == app)
            .context("Unknown app")?;
        let Some(source) = recipe.versions.as_ref().and_then(|p| p.source.as_ref()) else {
            return Ok(
                json!({"app":app,"available":false,"reason":"This recipe has no upstream release source."}),
            );
        };
        static CACHE: std::sync::OnceLock<std::sync::Mutex<BTreeMap<String, (u64, Value)>>> =
            std::sync::OnceLock::new();
        let cache = CACHE.get_or_init(Default::default);
        let cache_key = serde_json::to_string(&(app, &recipe.image, source))?;
        if let Some((time, value)) = cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Version cache unavailable"))?
            .get(&cache_key)
            && crate::core::now().saturating_sub(*time) < 300
        {
            return Ok(value.clone());
        }
        static ATTEMPTS: std::sync::OnceLock<std::sync::Mutex<BTreeMap<String, u64>>> =
            std::sync::OnceLock::new();
        {
            let mut attempts = ATTEMPTS
                .get_or_init(Default::default)
                .lock()
                .map_err(|_| anyhow::anyhow!("Version rate limiter unavailable"))?;
            ensure!(
                !attempts
                    .get(&source.repository)
                    .is_some_and(|time| crate::core::now().saturating_sub(*time) < 30),
                "Wait at least 30 seconds between upstream version checks"
            );
            attempts.insert(source.repository.clone(), crate::core::now());
        }
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .user_agent("obiente-selfhost-version-check")
            .build()?;
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page=30",
            source.repository
        );
        let mut response = None;
        for attempt in 0..3 {
            let result = client
                .get(&url)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send()
                .await?;
            if result.status().is_server_error() && attempt < 2 {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
                continue;
            }
            response = Some(result);
            break;
        }
        let mut response = response.context("Release discovery unavailable")?;
        ensure!(
            ![403, 429].contains(&response.status().as_u16()),
            "Upstream release API rate limited this request. Wait before checking again."
        );
        ensure!(
            response.status().is_success(),
            "Upstream release metadata unavailable (HTTP {})",
            response.status().as_u16()
        );
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            ensure!(
                body.len() + chunk.len() <= 2 * 1024 * 1024,
                "Release response exceeds 2 MiB"
            );
            body.extend_from_slice(&chunk);
        }
        let releases: Vec<Value> = serde_json::from_slice(&body)?;
        let current = recipe
            .image
            .rsplit_once(':')
            .and_then(|(_, s)| s.strip_prefix(&source.image_prefix))
            .and_then(stable);
        let mut candidates = vec![];
        for release in releases.iter().take(30) {
            if release["draft"] == true || release["prerelease"] == true {
                continue;
            }
            let Some(tag) = release["tag_name"].as_str() else {
                continue;
            };
            let Some(version) = tag.strip_prefix(&source.tag_prefix).and_then(stable) else {
                continue;
            };
            if current.is_some_and(|c| version <= c) {
                continue;
            }
            let stream = format!("{}.{}", version[0], version[1]);
            let compatible = stream == source.stream
                && current.is_some_and(|c| c[0] == version[0] && c[1] == version[1]);
            let image = format!(
                "{}:{}{}.{}.{}",
                image_repository(&recipe.image)?,
                source.image_prefix,
                version[0],
                version[1],
                version[2]
            );
            candidates.push(json!({"image":image,"tag":tag,"within_stream":compatible,"automatic_candidate":compatible&&source.automatic,"tested":false,"release_url":format!("https://github.com/{}/releases/tag/{}",source.repository,tag)}));
        }
        let result = json!({"app":app,"available":true,"current":recipe.image,"candidates":candidates,"checked_at":crate::core::now(),"limited_to_recent_releases":30,"note":"Release metadata is discovery only. Image availability, native configuration, migrations and smoke tests still require review."});
        cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Version cache unavailable"))?
            .insert(cache_key, (crate::core::now(), result.clone()));
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_selection_preserves_repository_and_requires_custom_ack() {
        let app = crate::catalog::load(None)
            .unwrap()
            .into_iter()
            .find(|a| a.id == "homarr")
            .unwrap();
        assert!(
            select(
                &app,
                &Selection {
                    image: "ghcr.io/homarr-labs/homarr:v9.0.0".into(),
                    allow_untested: false
                }
            )
            .is_err()
        );
        assert!(
            select(
                &app,
                &Selection {
                    image: "attacker/homarr:v1".into(),
                    allow_untested: true
                }
            )
            .is_err()
        );
        assert!(
            select(
                &app,
                &Selection {
                    image: "ghcr.io/homarr-labs/homarr:v9.0.0".into(),
                    allow_untested: true
                }
            )
            .is_ok()
        );
        for bad in ["redis", "registry:5000/app", "a:$(whoami)", "a@sha256:123"] {
            assert!(image_repository(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn strict_stable_streams_skip_prereleases_and_dates() {
        assert_eq!(stable("1.2.3"), Some([1, 2, 3]));
        for bad in ["v1.2.3", "1.2", "1.2.3-rc1", "01.2.3", "1.2.3+build"] {
            assert!(stable(bad).is_none());
        }
    }
    #[test]
    fn version_apply_rejects_stale_configuration_and_preserves_files() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path().into()).unwrap();
        let p = store
            .create(crate::core::CreateProject {
                name: "Versions".into(),
                apps: vec!["homarr".into()],
                server_id: "local".into(),
            })
            .unwrap();
        let choice = Selection {
            image: "ghcr.io/homarr-labs/homarr:v1.77.3".into(),
            allow_untested: true,
        };
        let plan = store.version_plan(&p.id, "homarr", &choice).unwrap();
        let mut setup = store.setup(&p.id).unwrap();
        setup
            .environment
            .insert("USER_VALUE".into(), "literal-$value".into());
        store.save_setup(&p.id, setup.clone()).unwrap();
        assert!(
            store
                .version_apply(&p.id, "homarr", &choice, plan["revision"].as_str().unwrap())
                .is_err()
        );
        let plan = store.version_plan(&p.id, "homarr", &choice).unwrap();
        store
            .version_apply(&p.id, "homarr", &choice, plan["revision"].as_str().unwrap())
            .unwrap();
        let after = store.setup(&p.id).unwrap();
        assert_eq!(after.environment, setup.environment);
        assert_eq!(after.files, setup.files);
        assert_eq!(
            after.compose["services"]["homarr"]["volumes"],
            setup.compose["services"]["homarr"]["volumes"]
        );
        assert_eq!(after.compose["services"]["homarr"]["image"], choice.image);
    }
}
