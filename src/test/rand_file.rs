#![allow(unused, dead_code, unused_variables)]

#[cfg(test)]
mod test {
    use crate::crypto_file;
    use crate::libs::load::strip_prefix;
    use md5::Digest;
    use rand::{Rng, RngCore};
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::AsyncWriteExt;
    use walkdir::WalkDir;

    async fn gen_rand_files(path: &str) {
        let mut r = rand::thread_rng();
        for i in 0..10 {
            let dir = gen();
            for i in 0..10 {
                let path = &format!("{}/{}", path, dir);
                tokio::fs::create_dir_all(path).await;

                let file_path = &format!("{}/{}", path, gen());

                let count = rand::thread_rng().gen_range(1..10 * 1024 * r.gen_range(5..46));
                let mut v = vec![0_u8; count];
                rand::thread_rng().fill_bytes(&mut v);

                let path = Path::new(file_path);

                let op = tokio::fs::File::create(file_path).await;
                if let Ok(mut file) = op {
                    file.write(&v).await;
                }
            }
        }
    }

    fn gen() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
            ^ rand::thread_rng().gen::<u64>()
    }

    #[tokio::main]
    async fn compare() {
        let path_from = "test/test_files";
        gen_rand_files(path_from).await;

        let path_to = "test_file_s";

        for file in WalkDir::new(path_from).into_iter().flatten() {
            let file_from_path = file.path();
            let file_to_path = Path::new(strip_prefix(path_from, file_from_path).to_str().unwrap());

            if file_from_path.is_file() {
                let val1 = crypto_file!(Path::new(file_from_path), md5::Md5::new())
                    .await
                    .unwrap();
                let val2 = crypto_file!(Path::new(file_to_path), md5::Md5::new())
                    .await
                    .unwrap();
                assert_eq!(val1, val2);
            }
        }
    }

    #[test]
    fn test_compare() {
        compare();
    }
}
