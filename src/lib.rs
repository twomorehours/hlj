use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingService, ServiceInstance,
};
use rand::seq::SliceRandom;
use reqwest::{Request, Response};
use reqwest_middleware::{ClientWithMiddleware, Middleware, Next, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use task_local_extensions::Extensions;
use tokio::sync::Mutex;
use tokio_cron_scheduler::Job as CronJob;
use tracing::{debug, error, info, warn};

pub struct NacosLoadBalancer<T> {
    client: T,
    group_name: String,
    services: Arc<Mutex<HashMap<String, Vec<ServiceInstance>>>>,
}

impl<T: NamingService> NacosLoadBalancer<T> {
    pub fn new(client: T, group_name: String) -> Self {
        NacosLoadBalancer {
            client,
            group_name,
            services: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn subscribe(&self, service_name: &str) -> anyhow::Result<()> {
        self.client
            .subscribe(
                service_name.to_owned(),
                Some(self.group_name.clone()),
                vec![],
                Arc::new(EventListener {
                    services: self.services.clone(),
                }),
            )
            .await
            .map_err(anyhow::Error::from)
    }
}

#[async_trait::async_trait]
impl<T: NamingService + Send + Sync + 'static> Middleware for NacosLoadBalancer<T> {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        println!("============================================");
        let url = req.url_mut();
        let service_name = url.host_str().unwrap();
        let mut services = self.services.lock().await;
        let ints = match services.get(service_name) {
            Some(instances) => instances,
            None => {
                let instances = self
                    .client
                    .select_instances(
                        service_name.to_owned(),
                        Some(self.group_name.clone()),
                        vec![],
                        true,
                        true,
                    )
                    .await
                    .map_err(anyhow::Error::from)?;
                error!("select instances: {:?}", instances);
                services.insert(service_name.to_owned(), instances);
                self.subscribe(service_name).await?;
                services.get(service_name).unwrap()
            }
        };
        // let mut rng = rand::thread_rng();
        match ints.get(0) {
            //todo rng
            Some(inst) => {
                url.set_host(Some(&inst.ip_and_port())).unwrap();
                next.run(req, extensions).await
            }
            None => Err(reqwest_middleware::Error::Middleware(anyhow::anyhow!(
                "no available instance"
            ))),
        }
    }
}

struct EventListener {
    services: Arc<Mutex<HashMap<String, Vec<ServiceInstance>>>>,
}

impl NamingEventListener for EventListener {
    fn event(&self, event: Arc<NamingChangeEvent>) {
        let services = self.services.clone();
        tokio::spawn(async move {
            if let Some(ints) = &event.instances {
                let mut services = services.lock().await;
                error!("receive naming events: {:?}", event);
                services.insert(
                    event.service_name.clone(),
                    ints.iter().filter(|i| i.healthy()).cloned().collect(),
                );
            }
        });
    }
}

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
        self.0
            .add(CronJob::new_async(
                job.cron.clone().as_str(),
                move |_uuid, mut _l| {
                    let job = job.clone();
                    let http_client = http_client.clone();
                    Box::pin(async move {
                        info!("start execute job: {}", job.name);
                        match job.run(http_client.clone()).await {
                            Ok(resp) => info!("execte job: {} success, result: {}", job.name, resp),
                            Err(err) => warn!("execte job: {} failed, result: {:?}", job.name, err),
                        }
                    })
                },
            )?)
            .await?;
        Ok(())
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        Ok(self.0.start().await?)
    }
}
