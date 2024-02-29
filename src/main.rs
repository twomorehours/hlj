use std::time::Duration;

use anyhow::Ok;
use hlj::NacosLoadBalancer;
use nacos_sdk::api::{
    naming::{NamingService, NamingServiceBuilder},
    props::ClientProps,
};
use reqwest::{Client, Request, Response};
use reqwest_middleware::{ClientBuilder, Middleware, Next, Result};
use task_local_extensions::Extensions;

struct LoggingMiddleware;

#[async_trait::async_trait]
impl Middleware for LoggingMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let url = req.url_mut();
        url.set_host(Some("baidu.com")).unwrap();
        // println!("Request started {:?}", req);
        next.run(req, extensions).await
        // println!("Result: {:?}", res);
        // res
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // let reqwest_client = Client::builder().build().unwrap();
    // let client = ClientBuilder::new(reqwest_client)
    //     .with(LoggingMiddleware)
    //     .build();
    // let resp = client.get("https://truelayer.com").send().await.unwrap();
    // println!("TrueLayer page HTML: {}", resp.text().await.unwrap());

    let naming_service = NamingServiceBuilder::new(
        ClientProps::new()
            .server_addr("localhost:8848")
            .app_name("simple_app"),
    )
    .build()?;

    let balancer = NacosLoadBalancer::new(naming_service);

    // loop {
    //     let instances = naming_service
    //         .select_instances(
    //             "mb-external-server".to_owned(),
    //             Some("DEFAULT_GROUP".to_owned()),
    //             vec![],
    //             true,
    //             true,
    //         )
    //         .await?;

    //     println!("{instances:?}");
    //     tokio::time::sleep(Duration::from_secs(10)).await;
    // }

    Ok(())
}
