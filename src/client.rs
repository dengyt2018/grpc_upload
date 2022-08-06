mod libs;
mod test;

use crate::grpc_upload_client::{client_run, ClientArgs};
use clap::Parser;
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = ClientArgs::parse();

    client_run(&args).await
}

pub mod grpc_upload_client {
    use crate::libs::load::{
        client_upload_file_request, upload_path, upload_request, UploadFilesTrait,
    };
    use clap::Parser;
    use std::path::Path;
    use std::process;

    use crate::libs::upload::upload_client::UploadClient;
    use crate::libs::upload::uploadfile_client::UploadfileClient;
    use crate::libs::utils::generate_connect::gen_client_channel;
    use crate::libs::utils::init_log::init_log;
    use crate::libs::utils::upload_queue::{QueueMethod, UploadQueue};
    use indicatif::ProgressBar;

    /// Simple client program to upload file to remote server use grpc
    /// For learn rust
    #[derive(Parser, Debug)]
    #[clap(author, version, about, long_about = None)]
    pub struct ClientArgs {
        /// The upload server http://www.example.com
        #[clap(short, long, default_value = "http://[::1]")]
        server: String,

        /// The upload server port
        #[clap(short, long, default_value_t = 50051)]
        port: u16,

        /// The upload directory
        #[clap(short, long)]
        dir: String,

        /// SSL Certificate path
        #[clap(short, long, default_value = "tls/ca.pem")]
        tls: String,

        /// Server Domain name, if have.
        #[clap(long)]
        domain: Option<String>,
    }

    pub async fn client_run(args: &ClientArgs) -> Result<(), Box<dyn std::error::Error>> {
        init_log();
        let server = format!("{}:{}", args.server, args.port);
        let channel = gen_client_channel(&server, &args.domain, &args.tls)
            .await
            .unwrap();

        if !Path::new(&args.dir).exists() {
            log::error!("Path: {} doesn't exists.", &args.dir);
            process::exit(-1);
        }

        let mut path = upload_path(&args.dir);
        let all_path = path.path();
        let all_path_length = all_path.len() as u64;

        let mut upload_client = UploadClient::new(channel.clone());
        let mut upload_file_client = UploadfileClient::new(channel);

        let mut upload_queue = UploadQueue::new();

        let bar = ProgressBar::new(all_path_length);

        for _ in 0..all_path_length {
            if let Some(mut data) = all_path.pop() {
                let upload_request = upload_request(&mut data).await;

                upload_queue.add(&upload_request);

                let request = tonic::Request::new(upload_request);
                let response = upload_client.upload(request).await?;
                let r = response.into_inner();

                if !r.require_upload {
                    upload_queue.remove(&r.queue);
                } else {
                    let file_data = upload_queue.info(&r.queue);
                    if let Ok(data) = file_data {
                        client_upload_file_request(
                            &r.queue,
                            data.to_owned(),
                            &mut upload_file_client,
                            &mut upload_queue,
                        )
                        .await;
                    };
                }
                bar.inc(1);
            }
        }

        Ok(())
    }
}
