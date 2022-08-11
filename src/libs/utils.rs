#![allow(unused, dead_code, unused_variables)]

pub mod uploadfile_info {
    use crate::libs::upload::uploadfile_info::FileInfo;
    use crate::libs::upload::{Data, UploadMode, UploadfileInfo};
    use crate::libs::utils::server_serialize_file::deserialize;
    use itertools::Itertools;

    pub trait UploadfileInfoTrait {
        fn add(&mut self, file_hash: &str, new_file_path: &str);
        fn new_file(&mut self, file_hash: &str, file_path: &str);
        fn check_file_is_uploaded(&self, file_hash: &str) -> Result<&String, ()>;
        fn remove_file(&mut self, file_hash: &str, data: &Data);
        fn load_serialize(&mut self, file: &str) -> Self;
    }

    impl UploadfileInfoTrait for UploadfileInfo {
        fn add(&mut self, file_hash: &str, new_file_path: &str) {
            let mut file_info = self.uploadfile_info.get_mut(file_hash);

            if let Some(mut info) = file_info {
                info.count_files += 1;
                info.file_path.push(new_file_path.to_owned());
                info.file_path.iter().cloned().unique().collect::<Vec<_>>();
            } else {
                self.uploadfile_info.insert(
                    file_hash.to_owned(),
                    FileInfo {
                        file_path: vec![new_file_path.to_owned()],
                        count_files: 1,
                    },
                );
            };
        }

        fn new_file(&mut self, file_hash: &str, file_path: &str) {
            self.uploadfile_info.insert(
                file_hash.to_owned(),
                FileInfo {
                    file_path: vec![file_path.to_owned()],
                    count_files: 1,
                },
            );
        }

        fn check_file_is_uploaded(&self, file_hash: &str) -> Result<&String, ()> {
            let file = self.uploadfile_info.get(file_hash);

            if let Some(file_info) = file {
                let f = file_info.file_path.get(0);
                if let Some(path) = f {
                    Ok(path)
                } else {
                    Err(())
                }
            } else {
                Err(())
            }
        }

        /// SYNCHRONIZE will remove file info
        fn remove_file(&mut self, file_hash: &str, data: &Data) {
            let mut file = self.uploadfile_info.get_mut(file_hash);

            if let Some(file_info) = file {
                let mut file_info: &mut Vec<_> = file_info.file_path.as_mut();
                for (index, path) in file_info.iter().enumerate() {
                    if data.file_path.eq_ignore_ascii_case(path) {
                        file_info.remove(index);
                        break;
                    }
                }
            }
        }

        fn load_serialize(&mut self, file: &str) -> Self {
            let uploadfile_info = deserialize(file);
            if let Ok(u) = uploadfile_info {
                log::info!("Load serialize file success.");
                u
            } else {
                log::info!("Load serialize file failed.");
                Self {
                    uploadfile_info: Default::default(),
                }
            }
        }
    }
}

pub mod upload_queue {
    use crate::libs::upload::{Data, UploadRequest};
    use rand::Rng;
    use std::collections::HashMap;
    use std::ops::{Deref, DerefMut};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::sync::Mutex;

    pub trait QueueMethod {
        fn new() -> Self;
        fn get(&self, queue: &u64) -> Option<&UploadRequest>;
        fn add(&mut self, upload_request: &UploadRequest);
        fn remove(&mut self, queue: &u64);
        fn generate_queue() -> u64;
        fn info(&mut self, queue: &u64) -> Result<&Data, ()>;
    }

    #[derive(Debug)]
    pub struct UploadQueue {
        pub upload_queue_count: u32,
        pub upload_queue: HashMap<u64, UploadRequest>,
    }

    impl QueueMethod for UploadQueue {
        fn new() -> Self {
            Self {
                upload_queue_count: 0,
                upload_queue: HashMap::new(),
            }
        }

        fn get(&self, queue: &u64) -> Option<&UploadRequest> {
            if self.upload_queue_count == 0 {
                return Option::None;
            }
            self.upload_queue.get(queue)
        }

        fn add(&mut self, upload_request: &UploadRequest) {
            let op = self
                .upload_queue
                .insert(upload_request.queue, upload_request.to_owned());

            self.upload_queue_count += 1;
        }

        fn remove(&mut self, queue: &u64) {
            let op = self.upload_queue.remove(queue);
            if let Some(o) = op {
                self.upload_queue_count -= 1;
            }
        }

        fn generate_queue() -> u64 {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64
                ^ rand::thread_rng().gen::<u64>()
        }

        fn info(&mut self, queue: &u64) -> Result<&Data, ()> {
            let upload_request = self.upload_queue.get(queue);
            if let Some(u) = upload_request {
                let data = u.data.as_ref().unwrap();
                Ok(data)
            } else {
                Err(())
            }
        }
    }
}

pub mod generate_connect {
    use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

    pub async fn gen_client_channel(
        address: &str,
        domain_name: &Option<String>,
        tls: &String,
    ) -> Result<Channel, Box<dyn std::error::Error + 'static>> {
        let pem = tokio::fs::read(tls.to_string()).await?;
        let ca = Certificate::from_pem(pem);

        let mut domain = "example.com".to_string();
        if let Some(d) = domain_name {
            domain = d.to_owned();
        }

        let tls = ClientTlsConfig::new()
            .ca_certificate(ca)
            .domain_name(domain);

        let address = Box::leak(address.to_owned().into_boxed_str());
        let channel = Channel::from_static(address)
            .tls_config(tls)?
            .connect()
            .await?;
        Ok(channel)
    }

    pub async fn gen_server_identity(
        pem: &str,
        key: &str,
    ) -> Result<Identity, Box<dyn std::error::Error>> {
        log::info!("Load tls identity file...");
        let cert = tokio::fs::read(pem).await?;
        let key = tokio::fs::read(key).await?;
        log::info!("Load tls identity file success.");
        Ok(Identity::from_pem(cert, key))
    }
}

pub mod server_serialize_file {
    use crate::libs::upload::UploadfileInfo;
    use prost::{DecodeError, Message};
    use std::error::Error;
    use std::fs;

    pub fn serialize(info: UploadfileInfo) -> Vec<u8> {
        let mut buf = Vec::new();
        info.encode(&mut buf);
        buf
    }

    pub fn serialize_to_file(proto: &[u8]) {
        fs::write(".upload_file_info.edb", proto);
    }

    pub fn deserialize(file_path: &str) -> Result<UploadfileInfo, ()> {
        let buf = fs::read(file_path);

        match buf {
            Ok(info) => {
                let upload_file_info = UploadfileInfo::decode(&info[..]);
                match upload_file_info {
                    Ok(info) => Ok(info),
                    Err(decode_error) => {
                        log::info!("Deserialize Error: {}", decode_error);
                        Err(())
                    }
                }
            }
            Err(err) => {
                log::info!("Deserialize File read Error: {}", err);
                Err(())
            }
        }
    }
}

pub mod server_to_do_action {
    use crate::libs::upload::{FileData, UploadMode, UploadfileInfo};
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::sync::mpsc::{Receiver, Sender};
    use tokio::sync::MutexGuard;

    pub struct ToDoQueue {
        action: Vec<Action>,
        mode: UploadMode,
    }

    pub struct Action {
        pub queue: u64,
        pub do_action: DoAction,
        pub from: String,
        pub to: String,
        pub file: Option<FileData>,
        pub file_hash: String,
    }
    pub enum DoAction {
        Delete,
        Copy,
        Move,
        Write,
    }

    pub trait ToDoActionTrait {
        fn new() -> Self;
        fn add(&mut self, action: Action);
        fn clear(&mut self);
    }

    impl Default for ToDoQueue {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ToDoActionTrait for ToDoQueue {
        fn new() -> Self {
            Self {
                action: vec![],
                mode: UploadMode::AddAndReplace,
            }
        }

        fn add(&mut self, action: Action) {
            log::debug!(
                "ToDoQueue Action insert. queue: {:?}, hash: {}, path: {}",
                &action.queue,
                &action.file_hash,
                &action.to,
            );
            self.action.push(action);
        }

        fn clear(&mut self) {
            self.action.clear();
            self.action = vec![];
        }
    }

    trait ActionTrait {
        fn new() -> Self;
    }

    impl ActionTrait for Action {
        fn new() -> Self {
            Self {
                queue: 0,
                do_action: DoAction::Write,
                from: "".to_string(),
                to: "".to_string(),
                file: None,
                file_hash: "".to_string(),
            }
        }
    }
}

pub mod chunk_method {
    pub enum ChunkMethod {
        Kb(usize),
        Mb(usize),
    }

    pub struct Chunk {
        pub buffer: usize,
        pub offset: u64,
        count: [usize; 3], // [count, total, file_size]
    }

    impl ChunkMethod {
        pub fn capacity_method(&self, file_size: usize) -> Chunk {
            const ONE_KB: usize = 1024_usize;
            const ONE_MB: usize = 1024_usize * 1024_usize;
            fn c(file_size: &usize, capacity: &usize) -> Chunk {
                Chunk {
                    buffer: if file_size > capacity {
                        *capacity
                    } else {
                        *file_size
                    },
                    offset: 0,
                    count: [0, 1 + (file_size / capacity), *file_size],
                }
            }
            match self {
                ChunkMethod::Kb(k) => c(&file_size, &(if *k == 0 { ONE_KB } else { k * ONE_KB })),
                ChunkMethod::Mb(m) => c(&file_size, &(if *m == 0 { ONE_MB } else { m * ONE_MB })),
            }
        }
    }

    impl Chunk {
        pub(crate) fn len(&self) -> u32 {
            self.count[1] as u32
        }
    }

    impl Iterator for Chunk {
        type Item = Chunk;

        fn next(&mut self) -> Option<Self::Item> {
            if self.count[0] == self.count[1] {
                None
            } else {
                let buffer = self.buffer;
                let mut c = Chunk {
                    buffer,
                    offset: (self.buffer * self.count[0]) as u64,
                    count: self.count,
                };
                if self.count[1] - 1 == self.count[0] {
                    c.buffer = self.count[2] - c.offset as usize;
                }
                self.count[0] += 1;
                Some(c)
            }
        }

        fn last(self) -> Option<Self::Item>
        where
            Self: Sized,
        {
            let offset = self.buffer * (self.count[1] - 1);
            Some(Chunk {
                buffer: self.count[2] - offset,
                offset: offset as u64,
                count: self.count,
            })
        }
    }
}

pub mod file_handle {
    use crate::libs::hash::checksum::md5_file;
    use crate::libs::upload::UploadfileInfo;
    use crate::libs::utils::uploadfile_info::UploadfileInfoTrait;
    use std::collections::HashMap;
    use std::ops::{Deref, DerefMut};
    use std::path::Path;
    use std::sync::Arc;
    use tokio::fs::File;
    use tokio::sync::Mutex;
    use tonic::async_trait;

    pub struct Handle {
        pub handle: tokio::fs::File,
        pub count_done: u32,
    }

    pub struct FileHandles {
        pub handle_store: HashMap<u64, Handle>,
        pub upload_file_info: Arc<Mutex<UploadfileInfo>>,
    }

    impl Deref for FileHandles {
        type Target = HashMap<u64, Handle>;

        fn deref(&self) -> &Self::Target {
            &self.handle_store
        }
    }

    impl DerefMut for FileHandles {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.handle_store
        }
    }

    #[tonic::async_trait]
    pub trait HandleTrait {
        fn new(upload_file_info: Arc<Mutex<UploadfileInfo>>) -> Self;
        fn remove(&mut self, queue: &u64);
        async fn add(&mut self, path: &Path, queue: &u64, total: &u32);
        fn handle(&mut self, queue: &u64) -> Result<&mut tokio::fs::File, ()>;
        async fn decrease(&mut self, queue: &u64, path: &str, file_hash: &str);
    }

    #[tonic::async_trait]
    impl HandleTrait for FileHandles {
        fn new(upload_file_info: Arc<Mutex<UploadfileInfo>>) -> Self {
            Self {
                handle_store: HashMap::new(),
                upload_file_info: Arc::new(Mutex::new(UploadfileInfo {
                    uploadfile_info: Default::default(),
                })),
            }
        }

        fn remove(&mut self, queue: &u64) {
            let op = self.handle_store.remove(queue);
        }

        async fn add(&mut self, path: &Path, queue: &u64, total: &u32) {
            if let Some(h) = self.handle_store.get(queue) {
                {}
            } else {
                if let Some(p) = path.parent() {
                    if !p.exists() {
                        tokio::fs::create_dir_all(p).await;
                    }
                }

                let mut result = tokio::fs::File::create(path).await;
                if let Ok(mut file) = result {
                    self.handle_store.insert(
                        *queue,
                        Handle {
                            handle: file,
                            count_done: *total,
                        },
                    );
                    log::debug!("queue: {}, The file handle has been generated, waiting for the file to be written: {:?}", queue, path);
                }
            };
        }

        fn handle(&mut self, queue: &u64) -> Result<&mut tokio::fs::File, ()> {
            let mut handle = self.handle_store.get_mut(queue);
            if let Some(h) = handle {
                Ok(&mut h.handle)
            } else {
                Err(())
            }
        }

        async fn decrease(&mut self, queue: &u64, file_path: &str, file_hash: &str) {
            let mut handle = self.handle_store.get_mut(queue);
            if let Some(h) = handle {
                h.count_done -= 1;
                if h.count_done == 0 {
                    self.remove(queue);
                    let mut upload_file_info = self.upload_file_info.lock().await;

                    if file_hash.eq_ignore_ascii_case(&*md5_file(file_path).await.unwrap()) {
                        log::info!("queue: {}, path: {} . write done.", queue, file_path);
                        upload_file_info.add(file_path, file_hash);
                    } else {
                        log::info!(
                            "queue: {}, path: {} . sync Failed. md5 hash didn't match.",
                            queue,
                            file_path
                        );
                    }
                }
            }
        }
    }
}

pub mod init_log {
    use chrono::Local;
    use env_logger::Builder;
    use log::LevelFilter;
    use std::io::Write;

    pub fn init_log() {
        Builder::new()
            .format(|buf, record| {
                writeln!(
                    buf,
                    "{} [{}] - {}",
                    Local::now().format("%Y-%m-%d%T%H:%M:%S"),
                    record.level(),
                    record.args()
                )
            })
            .filter(None, LevelFilter::Info)
            .init();
    }
}
