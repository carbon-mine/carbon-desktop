use std::{
    io::Read,
    time::{Duration, Instant},
};
use url::Url;

const HOSTS: [&str; 2] = ["piston-meta.mojang.com", "launchermeta.mojang.com"];

pub(super) fn allowed(url: &str, expected_path: &str) -> bool {
    HOSTS
        .iter()
        .any(|host| url == format!("https://{host}{expected_path}"))
}

fn redirect(current: &str, target: &str, expected_path: &str) -> Result<String, String> {
    let next = Url::parse(current)
        .and_then(|u| u.join(target))
        .map_err(|_| "Invalid metadata redirect.")?;
    if !allowed(next.as_str(), expected_path) {
        return Err("Mojang redirected metadata outside the approved HTTPS source or resource. Preparation stopped.".into());
    }
    Ok(next.to_string())
}

pub(super) fn fetch(start: &str, expected_path: &str, limit: u64) -> Result<Vec<u8>, String> {
    if !allowed(start, expected_path) {
        return Err("Unapproved Mojang metadata source.".into());
    }
    let method_timeout = Duration::from_secs(15);
    let deadline = Instant::now() + method_timeout;
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(method_timeout)
        .build()
        .map_err(|_| "Could not initialize HTTPS for Mojang metadata.")?;
    let mut url = start.to_owned();
    for hop in 0..=2 {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Mojang metadata timed out. Retry online.")?;
        let response = client
            .get(&url)
            .header("Accept", "application/json")
            .timeout(remaining)
            .send()
            .map_err(|_| {
                "Could not retrieve Mojang metadata over HTTPS. Check your connection and retry."
            })?;
        let status = response.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if hop == 2 {
                return Err("Too many Mojang metadata redirects. Retry later.".into());
            }
            let target = response
                .headers()
                .get("location")
                .and_then(|s| s.to_str().ok())
                .ok_or("Missing Mojang metadata redirect location.")?;
            url = redirect(&url, target, expected_path)?;
            continue;
        }
        if status != 200 {
            return Err(format!(
                "Mojang returned HTTP {status} for version metadata. Retry online later."
            ));
        }
        if response.content_length().is_some_and(|n| n > limit) {
            return Err("Mojang metadata exceeds its supported size.".into());
        }
        let mut bytes = Vec::new();
        response
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                "Mojang metadata was incomplete or timed out. Retry preparation online."
            })?;
        if bytes.len() as u64 > limit {
            return Err("Mojang metadata exceeds its supported size.".into());
        }
        return Ok(bytes);
    }
    Err("Too many metadata redirects.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approved_redirect_preserves_the_https_manifest_resource() {
        let path = crate::LIST_PATH;
        let base = format!("https://piston-meta.mojang.com{path}");
        let mirror = format!("https://launchermeta.mojang.com{path}");
        assert_eq!(redirect(&base, &mirror, path).unwrap(), mirror);
        for target in [
            "http://piston-meta.mojang.com",
            "https://example.org",
            "/other.json",
            "?q=x",
            "#outside",
            "https://user@piston-meta.mojang.com/mc/game/version_manifest_v2.json",
        ] {
            assert!(redirect(&base, target, path).is_err());
        }
    }
}
