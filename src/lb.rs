use nacos_sdk::api::naming::{
    NamingChangeEvent, NamingEventListener, NamingService, ServiceInstance,
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result};
use std::{collections::HashMap, sync::Arc};
use task_local_extensions::Extensions;
use tokio::sync::Mutex;

pub struct NacosLoadBalancer<T> {
    client: Arc<T>,
    group_name: String,
    services: Arc<Mutex<HashMap<String, Vec<ServiceInstance>>>>,
}

impl<T: NamingService> NacosLoadBalancer<T> {
    pub fn new(client: Arc<T>, group_name: String) -> Self {
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
                services.insert(service_name.to_owned(), instances);
                self.subscribe(service_name).await?;
                services.get(service_name).unwrap()
            }
        };

        match ints.choose(&mut StdRng::from_entropy()) {
            Some(inst) => {
                url.set_host(Some(inst.ip())).unwrap();
                url.set_port(Some(inst.port() as u16)).unwrap();
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
                services.insert(
                    event.service_name.clone(),
                    ints.iter().filter(|i| i.healthy()).cloned().collect(),
                );
            }
        });
    }
}
