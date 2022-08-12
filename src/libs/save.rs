#![allow(unused, dead_code, unused_variables)]

use crate::libs::upload::uploadfile_info::FileInfo;
use crate::libs::upload::{
    Chunk, Data, FileData, UploadFileRequest, UploadFileResponse, UploadMode, UploadRequest,
    UploadResponse, UploadfileInfo,
};
use crate::libs::utils::file_handle;
use crate::libs::utils::file_handle::{FileHandles, HandleTrait};
use crate::libs::utils::server_serialize_file::{serialize, serialize_to_file};
use crate::libs::utils::server_to_do_action::{Action, DoAction, ToDoActionTrait, ToDoQueue};
use crate::libs::utils::upload_queue::{QueueMethod, UploadQueue};
use crate::libs::utils::uploadfile_info::UploadfileInfoTrait;
use bytes::Buf;
use std::error::Error;
use std::io::{Cursor, SeekFrom};
use std::path::Path;
use std::ptr::hash;
use std::sync::Arc;
use tokio::io;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, MutexGuard};

pub async fn save_file(file_data: &FileData, file_handles: &mut FileHandles, queue: &u64) {
    let buffer = &file_data.file;
    let data = file_data.data.as_ref().unwrap();
    let chunk = data.chunk.as_ref().unwrap();
    let file_path = &data.file_path;
    let file_hash = &data.file_hash;
    let offset = chunk.offset;
    let path = Path::new(&file_path);

    file_handles.add(path, queue, &chunk.chunk_total).await;

    let result = file_handles.handle(queue);
    if let Ok(mut f) = result {
        f.seek(SeekFrom::Start(offset));
        let mut buf = std::io::Cursor::new(buffer);

        while buf.has_remaining() {
            f.write_buf(&mut buf).await;
        }
        log::debug!(
            "====> write queue: {}, path: {}, bytes: {}",
            queue,
            &file_path,
            &buffer.len()
        );
        file_handles.decrease(queue, file_path, file_hash).await;
    }
}

pub fn create_dir(dir_path: &String) {
    let path = Path::new(&dir_path);
    let result = std::fs::create_dir_all(path);
    if result.is_ok() {
        log::debug!("Directory created successfully: {:?}", path);
    } else {
        log::error!(
            "Failed to create directory. Please check permission. : {:?}",
            path
        );
    }
}

pub async fn upload_request(
    request: &UploadRequest,
    upload_file_info: &mut MutexGuard<'_, UploadfileInfo>,
    rx: &Sender<Action>,
) -> UploadResponse {
    let data = request.data.as_ref().unwrap();

    let mut require_upload = false;
    let upload_mode = match request.mode {
        0 => UploadMode::AddAndReplace,
        1 => UploadMode::UpdateAndAdd,
        2 => UploadMode::FreshenExisting,
        3 => UploadMode::Synchronize,
        _ => UploadMode::AddAndReplace,
    };

    if !data.is_file {
        create_dir(&data.file_path);
    } else {
        require_upload =
            !check_server_local_file(&upload_mode, &request.queue, data, upload_file_info, rx)
                .await;
    }

    UploadResponse {
        require_upload,
        queue: request.queue,
        file_hash: data.file_hash.to_owned(),
        chunk: None,
        is_chunk: false,
    }
}

async fn check_server_local_file(
    upload_mode: &UploadMode,
    queue: &u64,
    data: &Data,
    upload_file_info: &mut MutexGuard<'_, UploadfileInfo>,
    rx: &Sender<Action>,
) -> bool {
    let uploaded_path = upload_file_info.check_file_is_uploaded(&data.file_hash);

    if let Ok(server_path) = uploaded_path {
        log::debug!(
            "The file already exists on the server. queue: {}, file path: {} hash：{}",
            &queue,
            &data.file_path,
            &data.file_hash
        );

        rx.send(Action {
            queue: queue.to_owned(),
            do_action: DoAction::Copy,
            from: server_path.to_owned(),
            to: data.file_path.to_owned(),
            file: None,
            file_hash: data.file_hash.to_owned(),
        })
        .await;

        true
    } else {
        log::debug!(
            "The file could not be found on the server. queue: {}, file path: {} hash：{}",
            &queue,
            &data.file_path,
            &data.file_hash
        );
        false
    }
}

pub async fn server_upload_file_request(
    request: &UploadFileRequest,
    upload_file_info: &mut MutexGuard<'_, UploadfileInfo>,
    rx: &Sender<Action>,
) -> UploadFileResponse {
    let file_data = request.file_data.as_ref().unwrap();
    let data = file_data.data.as_ref().unwrap();
    let file_hash = &data.file_hash;
    let file_path = &data.file_path;

    rx.send(Action {
        queue: request.queue,
        do_action: DoAction::Write,
        from: "".to_string(),
        to: file_path.to_owned(),
        file: Option::Some(file_data.to_owned()),
        file_hash: file_hash.to_owned(),
    })
    .await;

    UploadFileResponse {
        queue: request.queue,
        upload_success: true,
        file_hash: file_hash.to_owned(),
        re_upload: false,
        chunk: data.chunk.to_owned(),
        is_chunk: data.is_chunk,
    }
}
