use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio_cron_scheduler::JobBuilder;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    id: String,
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

pub struct JobScheduler {
    inner: tokio_cron_scheduler::JobScheduler,
    jobs: HashMap<String, Job>,
    http_client: Arc<ClientWithMiddleware>,
}

impl JobScheduler {
    pub async fn new(http_client: Arc<ClientWithMiddleware>) -> Self {
        Self {
            inner: tokio_cron_scheduler::JobScheduler::new().await.unwrap(),
            jobs: HashMap::new(),
            http_client,
        }
    }

    pub fn add(&mut self, job: Job) {
        self.jobs.insert(job.id.clone(), job);
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        for (_id, raw_job) in self.jobs.clone().into_iter() {
            let http_client = self.http_client.clone();
            let job = JobBuilder::new()
                .with_timezone(chrono_tz::Asia::Shanghai)
                .with_cron_job_type()
                .with_schedule(raw_job.cron.clone().as_str())
                .unwrap()
                .with_run_async(Box::new(move |_uuid, mut _l| {
                    let raw_job = raw_job.clone();
                    let http_client = http_client.clone();
                    Box::pin(async move {
                        info!("start execute job: {}", raw_job.id.clone());
                        match raw_job.run(http_client.clone()).await {
                            Ok(resp) => info!(
                                "execute job: {} success, result: {}",
                                raw_job.id.clone(),
                                resp
                            ),
                            Err(err) => {
                                warn!("execute job: {} failed, result: {:?}", raw_job.id, err)
                            }
                        }
                    })
                }))
                .build()?;

            self.inner.add(job).await?;
        }

        Ok(self.inner.start().await?)
    }

    pub async fn trigger(&self, job_id: &str) -> anyhow::Result<()> {
        match self.jobs.get(job_id) {
            Some(job) => {
                let http_client = self.http_client.clone();
                let job = job.clone();
                tokio::spawn(async move {
                    info!("trigger execute job: {}", job.id.clone());
                    match job.run(http_client.clone()).await {
                        Ok(resp) => info!(
                            "trigger execute job: {} success, result: {}",
                            job.id.clone(),
                            resp
                        ),
                        Err(err) => {
                            warn!("trigger execute job: {} failed, result: {:?}", job.id, err)
                        }
                    }
                });
                Ok(())
            }
            None => Err(anyhow::anyhow!("job not found")),
        }
    }

    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        Ok(self.inner.shutdown().await?)
    }
}
