pub const DEFAULT_PORT: u16 = 7777;
pub const DEFAULT_SERVER_PATH: &str = "~/.local/bin/rnvim-server";
pub const GITHUB_REPO: &str = "GNITOAHC/remote-ssh.nvim";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GITHUB_API_BASE: &str = "https://api.github.com";

/// Maps `uname -sm` output to the release asset suffix.
pub fn platform_from_uname(s: &str) -> Option<&'static str> {
    match s.trim() {
        "Linux x86_64"  => Some("linux-x86_64"),
        "Linux aarch64" => Some("linux-aarch64"),
        "Darwin x86_64" => Some("darwin-x86_64"),
        "Darwin arm64"  => Some("darwin-aarch64"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_platforms() {
        assert_eq!(platform_from_uname("Linux x86_64"), Some("linux-x86_64"));
        assert_eq!(platform_from_uname("Linux aarch64"), Some("linux-aarch64"));
        assert_eq!(platform_from_uname("Darwin x86_64"), Some("darwin-x86_64"));
        assert_eq!(platform_from_uname("Darwin arm64"),  Some("darwin-aarch64"));
    }

    #[test]
    fn unknown_platform_returns_none() {
        assert_eq!(platform_from_uname("Windows x86_64"), None);
        assert_eq!(platform_from_uname(""), None);
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(platform_from_uname("  Linux x86_64\n"), Some("linux-x86_64"));
    }
}
