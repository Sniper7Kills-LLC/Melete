use journal_core::StrokePoint;

use crate::error::{Result, StorageError};

// Format: [version: u8][bincode-serialized Vec<StrokePoint>]. Version byte enables future migrations.
const VERSION_V1: u8 = 1;

pub fn pack_points(points: &[StrokePoint]) -> Result<Vec<u8>> {
    let body = bincode::serialize(points)?;
    let mut out = Vec::with_capacity(1 + body.len());
    out.push(VERSION_V1);
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn unpack_points(blob: &[u8]) -> Result<Vec<StrokePoint>> {
    let (version, body) = blob.split_first().ok_or(StorageError::EmptyBlob)?;
    match *version {
        VERSION_V1 => Ok(bincode::deserialize(body)?),
        other => Err(StorageError::UnsupportedBlobVersion(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64, p: f32) -> StrokePoint {
        StrokePoint {
            x,
            y,
            pressure: p,
            tilt_x: 0.0,
            tilt_y: 0.0,
            timestamp_ms: 12345,
        }
    }

    #[test]
    fn round_trip_points() {
        let points = vec![pt(0.0, 0.0, 0.5), pt(1.0, 2.0, 0.7), pt(3.0, 4.0, 1.0)];
        let blob = pack_points(&points).unwrap();
        assert_eq!(blob[0], VERSION_V1);
        let back = unpack_points(&blob).unwrap();
        assert_eq!(back, points);
    }

    #[test]
    fn empty_blob_errors() {
        let err = unpack_points(&[]).unwrap_err();
        assert!(matches!(err, StorageError::EmptyBlob));
    }

    #[test]
    fn bad_version_errors() {
        let err = unpack_points(&[99, 0, 0, 0]).unwrap_err();
        assert!(matches!(err, StorageError::UnsupportedBlobVersion(99)));
    }
}
