#![allow(unused, dead_code, unused_variables)]

pub mod checksum {

    ///
    /// # Examples
    /// ```
    /// use crate::{crypto_bytes, crypto_file};
    /// use md5::{Digest, Md5};
    /// use sha2::Sha256;
    /// use sm3::Sm3;
    ///
    /// let val = crypto_bytes!(b"hello world", Md5::new()).unwrap();
    /// assert_eq!("5eb63bbbe01eeed093cb22bb8f5acdc3", val);
    ///
    /// let val2 = crypto_bytes!(b"hello world", Sm3::new()).unwrap();
    /// assert_eq!(
    ///     "44f0061e69fa6fdfc290c494654a05dc0c053da7e5c52b84ef93a9d67d3fff88",
    ///     val2
    /// );
    ///
    /// let val3 = crypto_bytes!(b"hello world", Sha256::new()).unwrap();
    /// assert_eq!(
    ///     "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
    ///      val3
    /// );
    ///```
    #[macro_use]
    #[macro_export]
    macro_rules! crypto_bytes {
        ($bytes: expr, $algorithm: expr) => {{
            {
                use base16ct::Error::InvalidEncoding;
                use digest::core_api::{BufferKindUser, CoreWrapper};
                use md5::digest::{FixedOutput, Output};

                let mut hasher = $algorithm;
                hasher.update($bytes);
                let mut buf = [0_u8; 64];
                let result = base16ct::lower::encode_str(&hasher.finalize_fixed(), &mut buf);
                if let Ok(str) = result {
                    Ok(str.to_string())
                } else {
                    Err(InvalidEncoding)
                }
            }
        }};
    }

    #[macro_use]
    #[macro_export]
    macro_rules! crypto_file {
        ($path: expr, $algorithm: expr) => {{
            use base16ct::Error::InvalidEncoding;
            use digest::core_api::{BufferKindUser, CoreWrapper};
            use digest::{FixedOutput, Output};
            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            use $crate::libs::utils::chunk_method::ChunkMethod;

            let chunk_method = ChunkMethod::Mb(10).capacity_method($path);

            async {
                let result = tokio::fs::File::open($path).await;
                if let Ok(mut file) = result {
                    let mut buf = vec![0_u8; 64];
                    let mut hasher = $algorithm;

                    for c in chunk_method {
                        let mut buffer = vec![0_u8; c.buffer];
                        file.seek(tokio::io::SeekFrom::Start(c.offset))
                            .await
                            .expect("set seek failed.");
                        file.read_exact(&mut buffer)
                            .await
                            .expect("read file failed.");
                        hasher.update(buffer);
                    }

                    let result = base16ct::lower::encode_str(&hasher.finalize_fixed(), &mut buf);
                    if let Ok(str) = result {
                        Ok(str.to_string())
                    } else {
                        Err(InvalidEncoding)
                    }
                } else {
                    Err(InvalidEncoding)
                }
            }
        }};
    }
}

#[cfg(test)]
mod test {
    use crate::{crypto_bytes, crypto_file};
    use md5::{Digest, Md5};
    use rand::RngCore;
    use std::path::Path;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn test_md5_bytes() {
        let t = crypto_bytes!(b"hello world", Md5::new()).unwrap();
        assert_eq!("5eb63bbbe01eeed093cb22bb8f5acdc3", t);
    }

    #[test]
    fn test_md5_file() {
        #[tokio::main]
        async fn test() {
            let path = Path::new("tmp.eee");
            create_test_file(path).await;
            let val1 = "1BC28CBF89C43A35764C1EA888962576";
            let val2 = crypto_file!(path, Md5::new()).await.unwrap();

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
