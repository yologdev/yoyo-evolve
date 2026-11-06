//! The size cap on images the user names into a prompt (`--image`, `/add`,
//! `@path`): every door reads through `commands_file::read_image_for_add`,
//! which reads through [`read_image_capped`] here. Kept apart from
//! `commands_file.rs` because that file sits at its module-size ceiling.
//!
//! Before this cap the whole file was read and base64-encoded whatever its
//! size: a 300 MB `--image` measured 4.7 GB max RSS, and then the provider
//! rejected the request anyway. The limit and its strict `>` come from the
//! model-side twin, yoagent's `read_file`: since 0.27.0 its public
//! `tools::DEFAULT_MAX_IMAGE_BYTES` (3.75 MB, decimal), checked as
//! `metadata.len() > self.max_image_bytes`. Superseded claim (0.27 upgrade,
//! #1142): this was a copied private 20 MiB (`MAX_IMAGE_SIZE_BYTES`), so a
//! 4-20 MB `/add` was attached and then refused by the provider.
//! <!-- yoagent-version-claim: 0.27 -->

use crate::commands_file_fence::AddReadError;
use std::path::Path;

/// yoagent's own default for `read_file`, referenced, not copied. A file of
/// exactly this many bytes is ALLOWED and one byte more is refused, the same
/// strict `>` yoagent uses, so the model and the user get one verdict.
pub(crate) const MAX_IMAGE_BYTES: u64 = yoagent::tools::DEFAULT_MAX_IMAGE_BYTES;

/// The refusal for an image of `len` bytes, or `None` when it may be read.
/// Pure: the caller supplies the length from `metadata`, never the bytes.
pub(crate) fn image_too_large_message(path: &str, len: u64, cap: u64) -> Option<String> {
    if len <= cap {
        return None;
    }
    let path = crate::cli::sanitize_for_display(path);
    let mb = |b: u64| b as f64 / 1e6; // decimal MB, as yoagent prints it
    Some(format!(
        "{path} is too large to attach as an image ({:.1} MB, max {:.2} MB); \
         it was not read",
        mb(len),
        mb(cap)
    ))
}

/// Read an image's bytes, refusing (before any byte is read) a file larger
/// than [`MAX_IMAGE_BYTES`]. A path `metadata` cannot see skips the cap and
/// falls through to the read, so missing files keep their existing error;
/// FIFOs report length 0 and are refused by `read_regular` exactly as before.
pub(crate) fn read_image_capped(path: &str) -> Result<Vec<u8>, AddReadError> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.is_file() {
            if let Some(m) = image_too_large_message(path, meta.len(), MAX_IMAGE_BYTES) {
                return Err(AddReadError::Failed(m));
            }
        }
    }
    crate::non_regular_read::read_regular(Path::new(path))
        .map_err(|e| AddReadError::Failed(format!("failed to read {path}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands_file::read_image_for_add;
    use crate::config::DirectoryRestrictions;

    #[test]
    fn cap_boundary_table() {
        let cap = MAX_IMAGE_BYTES;
        // (len, refused?) — exactly the cap is allowed, matching yoagent's `>`.
        for (len, refused) in [(0, false), (cap - 1, false), (cap, false), (cap + 1, true)] {
            let got = image_too_large_message("a.png", len, cap);
            assert_eq!(got.is_some(), refused, "len {len}");
        }
    }

    #[test]
    fn message_names_path_size_and_cap() {
        let m = image_too_large_message("big.png", 30_000_000, MAX_IMAGE_BYTES).unwrap();
        assert_eq!(
            m,
            "big.png is too large to attach as an image (30.0 MB, max 3.75 MB); it was not read"
        );
    }

    #[test]
    fn message_sanitizes_control_bytes_in_path() {
        let m = image_too_large_message("ev\x1bil.png", 21 * 1024 * 1024, MAX_IMAGE_BYTES).unwrap();
        assert!(
            "ev\x1bil.png".as_bytes().contains(&0x1b),
            "fixture carries ESC"
        );
        assert!(!m.as_bytes().contains(&0x1b), "{m:?}");
    }

    #[test]
    fn read_image_for_add_refuses_oversized_sparse_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("big.png");
        // Sparse: set_len allocates no data blocks, so no multi-MB write.
        std::fs::File::create(&p)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES + 1)
            .unwrap();
        let ps = p.to_str().unwrap();
        let err = read_image_for_add(ps, &DirectoryRestrictions::default()).unwrap_err();
        assert_eq!(
            err,
            AddReadError::Failed(format!(
                "{ps} is too large to attach as an image (3.8 MB, max 3.75 MB); it was not read"
            ))
        );
    }

    #[test]
    fn real_read_path_agrees_with_yoagent_read_file_at_its_cap() {
        // Through the door every image path uses, with yoagent's public
        // const: exactly the cap reads, one byte over and the formerly
        // accepted 4 MB (under the old 20 MiB) are refused.
        let cap = yoagent::tools::DEFAULT_MAX_IMAGE_BYTES;
        let dir = tempfile::TempDir::new().unwrap();
        for (len, refused) in [(cap, false), (cap + 1, true), (4_000_000, true)] {
            let p = dir.path().join(format!("img{len}.png"));
            std::fs::File::create(&p).unwrap().set_len(len).unwrap();
            let got = read_image_for_add(p.to_str().unwrap(), &DirectoryRestrictions::default());
            assert_eq!(got.is_err(), refused, "len {len}: {:?}", got.err());
        }
    }

    #[test]
    fn read_image_for_add_small_file_still_reads_byte_identically() {
        use base64::Engine;
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("small.png");
        let body = b"\x89PNG\r\n\x1a\nfake";
        std::fs::write(&p, body).unwrap();
        let (data, mime) =
            read_image_for_add(p.to_str().unwrap(), &DirectoryRestrictions::default()).unwrap();
        assert_eq!(data, base64::engine::general_purpose::STANDARD.encode(body));
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn missing_file_keeps_its_existing_error() {
        let err = read_image_capped("/nonexistent/zz9.png").unwrap_err();
        match err {
            AddReadError::Failed(m) => assert!(
                m.starts_with("failed to read /nonexistent/zz9.png: "),
                "{m}"
            ),
            other => panic!("{other:?}"),
        }
    }
}
