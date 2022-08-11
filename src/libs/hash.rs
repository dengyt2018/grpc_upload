#![allow(unused, dead_code, unused_variables)]

pub mod checksum {
    use crate::libs::utils::chunk_method::ChunkMethod;
    use md5::digest::core_api::CoreWrapper;
    use md5::digest::{FixedOutput, Output};
    use md5::Md5;
    use md5::{Digest, Md5Core};
    use std::io::SeekFrom;
    use std::path::Path;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    pub async fn md5_file(path: &str) -> Result<String, ()> {
        let path = Path::new(path);
        let file_metadata = path.metadata();
        let mut file_size = 0_usize;
        if let Ok(mut metadata) = file_metadata {
            file_size = metadata.len() as usize;
        };
        let chunk_method = ChunkMethod::Mb(10).capacity_method(file_size);

        let mut result = tokio::fs::File::open(path).await;
        if let Ok(mut file) = result {
            let mut hasher = Md5::new();

            for c in chunk_method {
                let mut buffer = vec![0_u8; c.buffer];
                file.seek(SeekFrom::Start(c.offset)).await;
                file.read_exact(&mut buffer).await;
                hasher.update(buffer);
            }

            let hash = hasher.finalize_fixed();

            hash_to_hex(&hash)
        } else {
            Err(())
        }
    }
    pub fn md5_bytes(bytes: &[u8]) -> Result<String, ()> {
        let mut hasher = Md5::new();
        hasher.update(bytes);
        let hash = hasher.finalize_fixed();

        hash_to_hex(&hash)
    }

    fn hash_to_hex(hash: &Output<CoreWrapper<Md5Core>>) -> Result<String, ()> {
        let mut buf = [0_u8; 32];
        let hex_hash_result = base16ct::lower::encode_str(hash, &mut buf);
        if let Ok(hex_hash) = hex_hash_result {
            let hash = String::from(hex_hash);
            Ok(hash)
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod test {
    use crate::libs::hash::checksum::{md5_bytes, md5_file};
    use rand::RngCore;
    use std::path::Path;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn test_md5_bytes() {
        let t = md5_bytes(b"hello world").unwrap();
        assert_eq!("5eb63bbbe01eeed093cb22bb8f5acdc3", t);
    }

    #[test]
    fn test_md5_file() {
        #[tokio::main]
        async fn test() {
            let path = Path::new("tmp.eee");
            create_test_file(path).await;
            let val1 = "1BC28CBF89C43A35764C1EA888962576";
            let val2 = md5_file(path.to_str().unwrap()).await.unwrap();

            assert_eq!(val1.to_ascii_lowercase(), val2);

            tokio::fs::remove_file(path).await;
        }

        test();
    }

    async fn create_test_file(path: &Path) {
        if path.exists() {
            tokio::fs::remove_file(path).await;
        }
        let buffer = vec![101_u8; 1024 * 1023];
        let res = tokio::fs::File::create(path).await;
        if let Ok(mut file) = res {
            file.write(&buffer).await;
        }
    }
}
