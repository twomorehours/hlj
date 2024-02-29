use nacos_sdk::api::naming::{NamingService, ServiceInstance};
use rand::seq::SliceRandom;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};
use std::collections::HashMap;
use task_local_extensions::Extensions;
use tokio::sync::Mutex;

pub struct NacosLoadBalancer<T> {
    client: T,
    group_name: String,
    services: Mutex<HashMap<String, Vec<ServiceInstance>>>,
}

impl<T: NamingService> NacosLoadBalancer<T> {
    pub fn new(client: T, group_name: String) -> Self {
        NacosLoadBalancer {
            client,
            group_name,
            services: Mutex::new(HashMap::new()),
        }
    }

    async fn subscribe(&self, service_name: &str) -> anyhow::Result<()> {
        self.client.subscribe(
            service_name.to_owned(),
            Some(self.group_name.clone()),
            vec![],
            event_listener,
        );
        unimplemented!()
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
        let url = req.url_mut();
        let service_name = url.host_str().unwrap();
        let mut services = self.services.lock().await;
        let choosed: Option<&ServiceInstance> = match services.get(service_name) {
            Some(instances) => instances.choose(&mut rand::thread_rng()),
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
                unimplemented!()
            }
        };
        match choosed {
            Some(inst) => {
                url.set_host(Some("baidu.com")).unwrap();
                next.run(req, extensions).await
            }
            None => Err(reqwest_middleware::Error::Middleware(anyhow::anyhow!(
                "no available instance"
            ))),
        }
    }
}
