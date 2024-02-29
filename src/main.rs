use hlj::NacosLoadBalancer;
use nacos_sdk::api::{
    naming::{NamingService, NamingServiceBuilder},
    props::ClientProps,
};
use reqwest::{Client, Request, Response};
use reqwest_middleware::{ClientBuilder, Middleware, Next, Result};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::time;
use tokio_cron_scheduler::{ JobScheduler, JobSchedulerError};


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
    let client = ClientBuilder::new(reqwest_client)
        .with(NacosLoadBalancer::new(naming_service, "DEFAULT_GROUP".to_owned()))
        .build();

    // loop {
    //     let resp: serde_json::Value = client.get("http://mb-external-server:8083").send().await?.json().await?;
    //     println!("resp is {resp:?}");
    //     time::sleep(Duration::from_secs(10)).await;
    // }

    // let sched = JobScheduler::new().await?;

    // Add basic cron job
    // sched.add(
    //     Job::new("1/10 * * * * *", |_uuid, _l| {
    //         println!("I run every 10 seconds");
    //     })?
    // ).await?;

    // Add async job
    // sched
    //     .add(Job::new_async("1/7 * * * * *", |uuid, mut l| {
    //         Box::pin(async move {
    //             println!("I run async every 7 seconds");

    //             // Query the next execution time for this job
    //             let next_tick = l.next_tick_for_job(uuid).await;
    //             match next_tick {
    //                 Ok(Some(ts)) => println!("Next time for 7s job is {:?}", ts),
    //                 _ => println!("Could not get next tick for 7s job"),
    //             }
    //         })
    //     })?)
    //     .await?;

    // // Start the scheduler
    // sched.start().await?;

    // // Wait while the jobs run
    // tokio::time::sleep(Duration::from_secs(100)).await;

    let file_content = std::fs::read_to_string("testdata/jobs.yml")?;
    let jobs: Jobs =  serde_yaml::from_str(&file_content)?;

    
    let client = Arc::new(client);

    for j in jobs.jobs.iter() {
        let result =  j.run(client.clone()).await?;
        println!("res: {}", result);
    }
    

    Ok(())
}
