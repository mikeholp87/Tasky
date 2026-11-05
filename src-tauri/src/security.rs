//! Navigation policy for every WebView, including any added in the future.

pub fn navigation_allowed(url: &tauri::Url) -> bool {
    let origin = url.origin().ascii_serialization();
    matches!(
        origin.as_str(),
        "http://tauri.localhost" | "https://tauri.localhost"
    ) || (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (cfg!(debug_assertions) && origin == "http://localhost:1420")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permits_only_bundled_origins_and_the_debug_development_server() {
        for url in [
            "tauri://localhost/",
            "http://tauri.localhost/index.html#processes",
        ] {
            assert!(navigation_allowed(&url.parse().unwrap()));
        }
        for url in [
            "https://example.invalid/",
            "http://localhost:1421/",
            "file:///tmp/a.html",
            "javascript:alert(1)",
            "data:text/html,test",
            "https://tauri.localhost.evil.invalid/",
            "tauri://evil.invalid/",
        ] {
            assert!(!navigation_allowed(&url.parse().unwrap()), "{url}");
        }
        assert_eq!(
            navigation_allowed(&"http://localhost:1420/".parse().unwrap()),
            cfg!(debug_assertions)
        );
    }
}
