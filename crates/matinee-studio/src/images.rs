//! Image bytes, content types, and untrusted provider URLs.

use url::Url;

use crate::error::StudioError;

pub(crate) fn sniff_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("image/png", "png"))
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

pub(crate) fn content_type_matches(header: &str, sniffed: &str) -> bool {
    let header = header
        .split(';')
        .next()
        .unwrap_or(header)
        .trim()
        .to_ascii_lowercase();
    if header.is_empty() || header == "application/octet-stream" {
        return true;
    }
    let header = if header == "image/jpg" {
        "image/jpeg"
    } else {
        header.as_str()
    };
    header == sniffed
}

/// A provider-supplied image URL. HTTP(S) only, no userinfo, no other scheme.
pub fn validate_provider_image_url(value: &str) -> Result<Url, StudioError> {
    let url = Url::parse(value).map_err(|_| {
        StudioError::provider("The image provider returned an unusable image address.")
    })?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(StudioError::provider(
            "The image provider returned an image address Matinee will not fetch.",
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(StudioError::provider(
            "The image provider returned an image address that contains credentials.",
        ));
    }
    Ok(url)
}

pub(crate) fn same_http_origin(candidate: &Url, expected: &Url) -> bool {
    matches!(candidate.scheme(), "http" | "https")
        && candidate.scheme() == expected.scheme()
        && candidate.host_str() == expected.host_str()
        && candidate.port_or_known_default() == expected.port_or_known_default()
        && candidate.username().is_empty()
        && candidate.password().is_none()
}

pub(crate) fn extension_content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_supported_images_and_rejects_other_bytes() {
        assert_eq!(
            sniff_image(b"\x89PNG\r\n\x1a\nrest").unwrap().0,
            "image/png"
        );
        assert_eq!(
            sniff_image(&[0xFF, 0xD8, 0xFF, 0x00]).unwrap().0,
            "image/jpeg"
        );
        let mut webp = b"RIFF____WEBP".to_vec();
        webp[4..8].copy_from_slice(b"size");
        assert_eq!(sniff_image(&webp).unwrap().0, "image/webp");
        assert!(sniff_image(b"<html>").is_none());
        assert!(!content_type_matches("image/png", "image/jpeg"));
        assert!(content_type_matches(
            "image/jpeg; charset=binary",
            "image/jpeg"
        ));
    }

    #[test]
    fn provider_urls_reject_credentials_and_non_http_schemes() {
        assert!(validate_provider_image_url("https://cdn.example/poster.png").is_ok());
        assert!(validate_provider_image_url("http://cdn.example/poster.png").is_ok());
        assert!(validate_provider_image_url("https://user:secret@cdn.example/poster.png").is_err());
        assert!(validate_provider_image_url("file:///etc/passwd").is_err());
        assert!(validate_provider_image_url("javascript:alert(1)").is_err());
        let server = Url::parse("https://jellyfin.example:8920/base").unwrap();
        let valid = Url::parse("https://jellyfin.example:8920/Items/1/Images/Backdrop/0").unwrap();
        let wrong_port =
            Url::parse("https://jellyfin.example:9443/Items/1/Images/Backdrop/0").unwrap();
        assert!(same_http_origin(&valid, &server));
        assert!(!same_http_origin(&wrong_port, &server));
    }
}
