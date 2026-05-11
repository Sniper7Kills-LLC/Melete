use uuid::Uuid;

use crate::error::{Result, StorageError};

pub(crate) fn uuid_to_blob(id: Uuid) -> Vec<u8> {
    id.as_bytes().to_vec()
}

pub(crate) fn blob_to_uuid(blob: &[u8]) -> Result<Uuid> {
    if blob.len() != 16 {
        return Err(StorageError::InvalidUuid(blob.len()));
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(blob);
    Ok(Uuid::from_bytes(bytes))
}
