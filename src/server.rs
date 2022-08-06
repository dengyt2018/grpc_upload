mod libs;
mod test;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use crate::grpc_upload_server::{server_run, ServerArgs};
use crate::libs::save::{server_upload_file_request, upload_request};
use crate::libs::upload::upload_server::Upload;
use crate::libs::upload::uploadfile_server::Uploadfile;
use crate::libs::upload::UploadfileInfo;
use crate::libs::upload::{UploadFileRequest, UploadFileResponse, UploadRequest, UploadResponse};
use crate::libs::utils::server_to_do_action::Action;
use clap::Parser;
use mimalloc::MiMalloc;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

pub trait UploadSender {
    fn new(rx_write: Sender<Action>, upload_file_info: Arc<Mutex<UploadfileInfo>>) -> Self;
}

pub struct DoUpload {
    pub rx_write: Sender<Action>,
    pub upload_file_info: Arc<Mutex<UploadfileInfo>>,
}

impl UploadSender for DoUpload {
    fn new(rx_write: Sender<Action>, upload_file_info: Arc<Mutex<UploadfileInfo>>) -> Self {
        Self {
            rx_write,
            upload_file_info,
        }
    }
}

pub struct DoUploadFile {
    pub rx_write: Sender<Action>,
    pub upload_file_info: Arc<Mutex<UploadfileInfo>>,
}

impl UploadSender for DoUploadFile {
    fn new(rx_write: Sender<Action>, upload_file_info: Arc<Mutex<UploadfileInfo>>) -> Self {
        Self {
            rx_write,
            upload_file_info,
        }
    }
}

#[tonic::async_trait]
impl Upload for DoUpload {
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        let req = request.into_inner();
        log::debug!("DoUpload Got message from client. queue is: {}", &req.queue);

        let mut upload_file_info = self.upload_file_info.lock().await;
        log::debug!("DoUpload got UploadfileInfo.lock. queue is: {}", &req.queue);

        let rx_write = &self.rx_write.clone();

        let reply = upload_request(&req, &mut upload_file_info, rx_write).await;

        log::debug!("DoUpload got UploadResponse queue is: {}", &req.queue);
        log::debug!("UploadResponse: {:?}", &reply);

        Ok(Response::new(reply))
    }
}

#[tonic::async_trait]
impl Uploadfile for DoUploadFile {
    async fn upload_file(
        &self,
        request: Request<UploadFileRequest>,
    ) -> Result<Response<UploadFileResponse>, Status> {
        let req = request.into_inner();
        log::debug!(
            "DoUploadFile Got message from client. queue is: {}",
            &req.queue
        );

        let mut upload_file_info = self.upload_file_info.lock().await;
        log::debug!(
            "DoUploadFile got UploadfileInfo.lock. queue is: {}",
            &req.queue
        );

        let rx_write = &self.rx_write.clone();

        let reply = server_upload_file_request(&req, &mut upload_file_info, rx_write).await;
        log::debug!("DoUploadFile got UploadResponse queue is: {}", &req.queue);
        log::debug!("UploadFileResponse: {:?}", &reply);

        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ServerArgs::parse();
    server_run(&args).await
}

pub mod grpc_upload_server {
    use crate::libs::save::save_file;
    use crate::libs::upload::upload_server::UploadServer;
    use crate::libs::upload::uploadfile_server::UploadfileServer;
    use crate::libs::utils::file_handle::{FileHandles, HandleTrait};
    use crate::libs::utils::generate_connect::gen_server_identity;
    use crate::libs::utils::init_log::init_log;
    use crate::libs::utils::server_to_do_action::DoAction;
    use crate::libs::utils::uploadfile_info::UploadfileInfoTrait;
    use crate::{DoUpload, DoUploadFile, UploadSender, UploadfileInfo};
    use clap::Parser;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use tonic::transport::Server;
    use tonic::transport::ServerTlsConfig;

    /// Simple server program to receive file from client server use grpc
    /// For learn rust
    #[derive(Parser, Debug)]
    #[clap(author, version, about, long_about = None)]
    pub struct ServerArgs {
        /// The upload server listen address
        #[clap(short, long, default_value = "[::1]")]
        address: String,

        /// The upload server listen port
        #[clap(short, long, default_value_t = 50051)]
        port: u16,

        /// SSL Certificate public key
        #[clap(long, default_value = "tls/server.pem")]
        pem: String,

        /// SSL Certificate private key
        #[clap(long, default_value = "tls/server.key")]
        key: String,
    }

    pub async fn server_run(args: &ServerArgs) -> Result<(), Box<dyn std::error::Error>> {
        init_log();

        let addr = format!("{}:{}", args.address, args.port)
            .parse()
            .expect("Invalid address, please reconfirm it.");

        let identity = gen_server_identity(&args.pem, &args.key).await?;

        let (rx_write, mut tx_write) = tokio::sync::mpsc::channel(10);

        let upload_file_info = Arc::new(Mutex::new(UploadfileInfo::default()));
        let upload_file_info2 = upload_file_info.clone();

        let server = tokio::spawn(async move {
            log::info!("UploadServer listening on {}", addr);
            Server::builder()
                .tls_config(ServerTlsConfig::new().identity(identity))?
                .add_service(UploadServer::new(DoUpload::new(
                    rx_write.clone(),
                    upload_file_info.clone(),
                )))
                .add_service(UploadfileServer::new(DoUploadFile::new(
                    rx_write,
                    upload_file_info.clone(),
                )))
                .serve(addr)
                .await
        });

        let mut file_handles = FileHandles::new(upload_file_info2);

        let action_write_file = tokio::spawn(async move {
            while let Some(ac) = tx_write.recv().await {
                match ac.do_action {
                    DoAction::Write => {
                        if let Some(file_data) = &ac.file {
                            save_file(file_data, &mut file_handles, &ac.queue).await
                        }
                    }
                    DoAction::Delete => {}
                    DoAction::Copy => {
                        let result = tokio::fs::copy(&ac.from, &ac.to).await;
                        if let Ok(bytes) = result {
                            let mut file_info = file_handles.upload_file_info.lock().await;
                            file_info.add(&ac.file_hash, &ac.to);
                            log::info!(
                                "queue: {}, File copy success. {}, bytes: {}",
                                ac.queue,
                                &ac.to,
                                bytes
                            );
                        } else {
                            log::error!(
                                "queue: {}, File can't copy. from: {} to: {}",
                                ac.queue,
                                &ac.from,
                                &ac.to
                            );
                        }
                    }
                    DoAction::Move => {}
                }
            }
        });

        action_write_file.await?;
        server.await?.expect("Run Server Crash...");
        Ok(())
    }
}
