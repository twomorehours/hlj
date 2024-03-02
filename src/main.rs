use clap::Parser;
use hlj::{job::JobScheduler, lb::NacosLoadBalancer};
use local_ip_address::local_ip;
use nacos_sdk::api::{
    naming::{NamingService, NamingServiceBuilder, ServiceInstance},
    props::ClientProps,
};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use time::macros::offset;
use tracing_subscriber::fmt::time::OffsetTime;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Jobs {
    jobs: Vec<hlj::job::Job>,
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[arg(long)]
    nacos_addr: String,
    #[arg(long)]
    nacos_group: String,
    #[arg(long)]
    job_conf_path: PathBuf,
    #[arg(long, default_value = "50700")]
    listen_port: u16,
    #[arg(long, default_value = "3")]
    req_timeout_secs: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let timer = OffsetTime::new(offset!(+8), time::format_description::well_known::Rfc3339);
    tracing_subscriber::fmt().with_timer(timer).init();
    let cli = Cli::parse();

    let naming_service = Arc::new(
        NamingServiceBuilder::new(
            ClientProps::new()
                .server_addr(cli.nacos_addr)
                .naming_push_empty_protection(false),
        )
        .build()?,
    );

    let mut instance = ServiceInstance::default();
    let local_ip = local_ip().unwrap();
    instance.ip = local_ip.to_string();
    instance.port = cli.listen_port as i32;
    naming_service
        .register_instance(
            "hlj".to_string(),
            Some(cli.nacos_group.clone()),
            instance.clone(),
        )
        .await?;

    let reqwest_client = Client::builder()
        .timeout(Duration::from_secs(cli.req_timeout_secs))
        .build()
        .unwrap();
    let client = Arc::new(
        ClientBuilder::new(reqwest_client)
            .with(NacosLoadBalancer::new(
                naming_service.clone(),
                cli.nacos_group.clone(),
            ))
            .build(),
    );

    let file_content = std::fs::read_to_string(cli.job_conf_path)?;
    let jobs: Jobs = serde_yaml::from_str(&file_content)?;

    let mut js = JobScheduler::new().await;

    for job in jobs.jobs {
        js.new_async_job(job, client.clone()).await?;
    }

    js.start().await?;

    tokio::signal::ctrl_c().await?;

    naming_service
        .deregister_instance("hlj".to_string(), Some(cli.nacos_group), instance)
        .await?;

    Ok(())
}
