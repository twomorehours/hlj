use hlj::{JobScheduler, NacosLoadBalancer};
use nacos_sdk::api::{naming::NamingServiceBuilder, props::ClientProps};
use reqwest::Client;
use reqwest_middleware::ClientBuilder;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Jobs {
    jobs: Vec<hlj::Job>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let naming_service = NamingServiceBuilder::new(
        ClientProps::new()
            .server_addr("localhost:8848")
            .app_name("simple_app"),
    )
    .build()?;

    let reqwest_client = Client::builder().build().unwrap();
    let client = Arc::new(
        ClientBuilder::new(reqwest_client)
            .with(NacosLoadBalancer::new(
                naming_service,
                "DEFAULT_GROUP".to_owned(),
            ))
            .build(),
    );

    let file_content = std::fs::read_to_string("testdata/jobs.yml")?;
    let jobs: Jobs = serde_yaml::from_str(&file_content)?;

    let mut js = JobScheduler::new().await;

    for job in jobs.jobs {
        js.new_async_job(job, client.clone()).await?;
    }

    js.start().await?;

    tokio::signal::ctrl_c().await?;

    Ok(())
}
