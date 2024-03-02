use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_cron_scheduler::JobBuilder;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    name: String,
    cron: String,
    method: String,
    url: String,
    content_type: String,
    body: Option<String>,
}

impl Job {
    pub async fn run(&self, http_client: Arc<ClientWithMiddleware>) -> anyhow::Result<String> {
        match self.method.to_lowercase().as_str() {
            "get" => {
                let resp: serde_json::Value = http_client
                    .get(&self.url)
                    .header("Content-Type", &self.content_type)
                    .send()
                    .await?
                    .json()
                    .await?;
                Ok(serde_json::to_string(&resp)?)
            }
            "post" => {
                let mut builer = http_client
                    .post(&self.url)
                    .header("Content-Type", &self.content_type);
                if let Some(body) = &self.body {
                    builer = builer.body(body.clone().into_bytes());
                }
                let resp: serde_json::Value = builer.send().await?.json().await?;
                Ok(serde_json::to_string(&resp)?)
            }
            _ => Err(anyhow::anyhow!("unsupport method: {}", self.method)),
        }
    }
}

pub struct JobScheduler(tokio_cron_scheduler::JobScheduler);

impl JobScheduler {
    pub async fn new() -> Self {
        Self(tokio_cron_scheduler::JobScheduler::new().await.unwrap())
    }

    pub async fn new_async_job(
        &mut self,
        job: Job,
        http_client: Arc<ClientWithMiddleware>,
    ) -> anyhow::Result<()> {
        let job = JobBuilder::new()
            .with_timezone(chrono_tz::Asia::Shanghai)
            .with_cron_job_type()
            .with_schedule(job.cron.clone().as_str())
            .unwrap()
            .with_run_async(Box::new(move |_uuid, mut _l| {
                let job = job.clone();
                let http_client = http_client.clone();
                Box::pin(async move {
                    info!("start execute job: {}", job.name);
                    match job.run(http_client.clone()).await {
                        Ok(resp) => info!("execute job: {} success, result: {}", job.name, resp),
                        Err(err) => warn!("execute job: {} failed, result: {:?}", job.name, err),
                    }
                })
            }))
            .build()?;

        self.0.add(job).await?;
        Ok(())
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(self.0.start().await?)
    }
}
