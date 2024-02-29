use clap::Parser;
use hlj::{JobScheduler, NacosLoadBalancer};
use nacos_sdk::api::{naming::NamingServiceBuilder, props::ClientProps};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Jobs {
    jobs: Vec<hlj::Job>,
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let naming_service = NamingServiceBuilder::new(
        ClientProps::new()
            .server_addr(cli.nacos_addr)
            .naming_push_empty_protection(false)
            .app_name("huanglongjiang"),
    )
    .build()?;

    let reqwest_client = Client::builder().build().unwrap();
    let client = Arc::new(
        ClientBuilder::new(reqwest_client)
            .with(NacosLoadBalancer::new(naming_service, cli.nacos_group))
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

    Ok(())
}
