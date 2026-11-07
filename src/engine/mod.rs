//! Playback engine — libmpv behind a small, independent wrapper.

pub mod ffi;
pub mod mpv;

pub use mpv::{ChapterInfo, EndReason, EngineEvent, MpvPlayer, ScreenshotRequest, TrackInfo, TrackKind};

/// Where media comes from. Paths are stored as given; the engine passes them
/// to libmpv verbatim.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MediaSource {
    Url(String),
    Path(String),
}

impl MediaSource {
    /// String handed to the engine (URL or filesystem path).
    pub fn resource(&self) -> &str {
        match self {
            MediaSource::Url(u) => u,
            MediaSource::Path(p) => p,
        }
    }

    /// True for network sources (history stores the raw URL text for these).
    pub fn is_url(&self) -> bool {
        matches!(self, MediaSource::Url(_))
    }
}

/// Compute the display title for a source, reproducing the documented
/// derivation (docs/01): local files show the file name without extension;
/// URL sources show the URL tail with the extension stripped and
/// percent-escape sequences *dropped* (observed quirk: `Sample%20B.mp4`
/// renders as `SampleB`).
pub fn display_title(source: &MediaSource) -> String {
    let raw = match source {
        MediaSource::Url(u) => {
            // URL tail: everything after the last '/', ignoring any query.
            let path = u.split(['?', '#']).next().unwrap_or(u);
            let tail = path.rsplit('/').next().unwrap_or(path);
            strip_extension_and_drop_escapes(tail)
        }
        MediaSource::Path(p) => {
            let name = std::path::Path::new(p)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(p.as_str());
            strip_extension_and_drop_escapes(name)
        }
    };
    if raw.is_empty() {
        source.resource().to_string()
    } else {
        raw
    }
}

fn strip_extension_and_drop_escapes(name: &str) -> String {
    let no_ext = match name.rfind('.') {
        // Keep dots that are part of a leading dotfile ("." / "..").
        Some(pos) if pos > 0 && !name[..pos].is_empty() => &name[..pos],
        _ => name,
    };
    // Drop percent-escape sequences rather than decoding them (documented
    // behavior: the encoded space in `Sample%20B.mp4` was dropped, not
    // restored).
    let mut out = String::with_capacity(no_ext.len());
    let mut chars = no_ext.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: Option<String> = {
                let a = chars.peek().copied();
                let mut it = chars.clone();
                it.next();
                let b = it.peek().copied();
                match (a, b) {
                    (Some(a), Some(b)) if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() => {
                        Some(format!("{a}{b}"))
                    }
                    _ => None,
                }
            };
            if hex.is_some() {
                chars.next();
                chars.next();
                continue; // drop the escape entirely
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> MediaSource {
        MediaSource::Url(s.to_string())
    }

    #[test]
    fn title_local_file() {
        assert_eq!(display_title(&MediaSource::Path("/movies/Sample A.mp4".into())), "Sample A");
    }

    #[test]
    fn title_url_drops_encoded_space() {
        // Documented quirk (docs/01): encoded space is dropped, not restored.
        assert_eq!(
            display_title(&url("http://127.0.0.1:8069/Sample%20B.mp4")),
            "SampleB"
        );
    }

    #[test]
    fn title_url_plain_name() {
        assert_eq!(
            display_title(&url("https://example.org/videos/movie.mkv")),
            "movie"
        );
    }

    #[test]
    fn title_url_query_ignored() {
        assert_eq!(
            display_title(&url("http://x.y/film.webm?token=ab%20cd")),
            "film"
        );
    }

    #[test]
    fn title_no_extension() {
        assert_eq!(display_title(&MediaSource::Path("/a/b/just-a-name".into())), "just-a-name");
        assert_eq!(display_title(&url("http://x.y/noext")), "noext");
    }

    #[test]
    fn title_dotfile() {
        assert_eq!(display_title(&MediaSource::Path("/a/.hidden.mp4".into())), ".hidden");
    }

    #[test]
    fn title_incomplete_escape_kept() {
        assert_eq!(display_title(&url("http://x.y/a%2.mp4")), "a%2");
        assert_eq!(display_title(&url("http://x.y/a%.mp4")), "a%");
    }

    #[test]
    fn title_multi_escape() {
        assert_eq!(display_title(&url("http://x.y/a%20b%20c.mp4")), "abc");
    }
}
