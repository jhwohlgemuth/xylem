use super::{io, AcquisitionError, Digest, Path, Sha256, SystemTime, UNIX_EPOCH};

pub(super) fn io_error(action: &'static str, path: impl AsRef<Path>, source: io::Error) -> AcquisitionError {
    AcquisitionError::Io {
        action,
        path: path.as_ref().to_path_buf(),
        source,
    }
}
pub(super) fn now_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
