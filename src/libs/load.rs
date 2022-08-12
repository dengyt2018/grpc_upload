#![allow(unused, dead_code, unused_variables)]

use crate::libs::upload;
use crate::libs::upload::uploadfile_client::UploadfileClient;
use crate::libs::upload::{
    Chunk, Data, FileData, Metadata, UploadFileRequest, UploadMode, UploadRequest, UploadResponse,
};
use crate::libs::utils::chunk_method::ChunkMethod;
use crate::libs::utils::upload_queue::{QueueMethod, UploadQueue};
use crate::{crypto_bytes, crypto_file, libs};
use log::log;
use md5::Digest;
use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::io::Seek;
use std::io::{Error, Read, SeekFrom};
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncSeekExt;
use tokio::sync::mpsc::channel;
use tonic::codegen::Body;
use tonic::transport::Channel;
use walkdir::WalkDir;

pub async fn load_file(file_handle: &mut tokio::fs::File, buffer: &usize, offset: &u64) -> Vec<u8> {
    let mut buf = vec![0_u8; *buffer as usize];

    file_handle.seek(SeekFrom::Start(*offset)).await;
    file_handle.read_exact(&mut buf).await;

    buf
}

pub trait UploadFilesTrait {
    fn path(&mut self) -> &mut Vec<Data>;
}

#[derive(Debug)]
pub struct UploadFiles {
    pub files: Vec<Data>,
}

impl UploadFilesTrait for UploadFiles {
    fn path(&mut self) -> &mut Vec<Data> {
        &mut self.files
    }
}

pub fn upload_path(file_path: &str) -> UploadFiles {
    let mut files = Vec::new();

    for file in WalkDir::new(file_path).into_iter().flatten() {
        let p = file.path();
        let mut metadata = Option::Some(libs::upload::Metadata {
            creation_time: 0,
            last_access_time: 0,
            last_write_time: 0,
        });
        let mut file_size = 0;
        if let Ok(m) = p.metadata() {
            file_size = m.len();
            metadata = Option::Some(libs::upload::Metadata {
                creation_time: m.creation_time(),
                last_access_time: m.last_access_time(),
                last_write_time: m.last_write_time(),
            });
        }
        let file_path = path(strip_prefix(file_path, p));

        files.push(Data {
            queue: 0,
            file_hash: "".to_string(),
            file_path,
            file_name: file_name(p),
            is_file: p.is_file(),
            file_size,
            metadata,
            is_chunk: false,
            original_path: path(p),
            chunk: None,
        });
    }

    UploadFiles { files }
}

/// Returns the `&Path` strip prefix.
///
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// let prefix = "./abc/def/target";
/// let path = Path::new("./abc/def/target/hello");
///
/// assert_eq!("./target/hello", strip_prefix(prefix, path));
/// ```
pub fn strip_prefix<'a>(prefix: &'a str, path: &'a Path) -> &'a Path {
    let p = Path::new(prefix).parent().unwrap().to_str().unwrap();
    if p.is_empty() {
        return path;
    }
    path.strip_prefix(p).unwrap_or(path)
}

fn file_name(path: &Path) -> String {
    if path.is_file() {
        path.file_name().unwrap().to_str().unwrap().to_string()
    } else {
        "".to_string()
    }
}

fn path(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

pub async fn upload_request(data: &mut Data) -> UploadRequest {
    let queue = UploadQueue::generate_queue();
    data.queue = queue.to_owned();
    data.file_hash = if data.is_file {
        if let Ok(hash) = crypto_file!(Path::new(&data.original_path), md5::Md5::new()).await {
            hash
        } else {
            "".to_string()
        }
    } else {
        "".to_string()
    };
    UploadRequest {
        queue,
        data: Option::Some(data.to_owned()),
        mode: UploadMode::AddAndReplace as i32,
    }
}

pub async fn client_upload_file_request(
    queue: &u64,
    data: Data,
    upload_file_client: &mut UploadfileClient<Channel>,
    upload_queue: &mut UploadQueue,
) {
    let chunk_method = ChunkMethod::Mb(10).capacity_method(Path::new(&data.original_path));
    let chunk_total = chunk_method.len();
    let (tx, mut rx) = channel(5);
    let file_hash = data.file_hash.to_owned();
    let file_path = data.file_path.to_owned();

    tokio::spawn(async move {
        let mut result = tokio::fs::File::open(&data.original_path).await;

        if let Ok(mut file) = result {
            for c in chunk_method {
                let file = load_file(&mut file, &c.buffer, &c.offset).await;

                let mut chunk_hash = "".to_string();
                if let Ok(h) = crypto_bytes!(&file, md5::Md5::new()) {
                    chunk_hash = h;
                }

                let mut data = data.to_owned();

                data.chunk = Option::Some(Chunk {
                    chunk_queue: 0,
                    chunk_total,
                    chunk_hash,
                    offset: c.offset,
                    buffer: c.buffer as u64,
                });

                let file_data = FileData {
                    file,
                    data: Option::from(data),
                };

                tx.send(file_data).await.unwrap();
            }
        }
    });

    while let Some(file_data) = rx.recv().await {
        let file_bytes = file_data.file.len();

        let upload_file_request = UploadFileRequest {
            queue: *queue,
            file_hash: file_hash.clone(),
            file_data: Option::Some(file_data),
        };

        let file_request = tonic::Request::new(upload_file_request.to_owned());
        let file_response = upload_file_client.upload_file(file_request).await;
        if let Ok(response) = file_response {
            let r = response.into_inner();
            if r.upload_success && !r.re_upload {
                log::info!(
                    "File uploaded successfully. queue: {}, hash: {}, path: {}, bytes: {}",
                    queue,
                    file_hash,
                    file_path,
                    file_bytes,
                );
            }
        };
    }
    upload_queue.remove(queue);
}
