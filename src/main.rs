use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use clap::Parser;
use hlj::{
    lb::NacosLoadBalancer,
    scheduler::{Job, JobScheduler},
};
use local_ip_address::local_ip;
use nacos_sdk::api::{
    naming::{NamingService, NamingServiceBuilder, ServiceInstance},
    props::ClientProps,
};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use time::macros::offset;
use tokio::{net::TcpListener, sync::Mutex};
use tracing::{info, warn};
use tracing_subscriber::fmt::time::OffsetTime;

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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Jobs {
    jobs: Vec<hlj::scheduler::Job>,
}

#[derive(Clone)]
struct AppState {
    path: PathBuf,
    js: Arc<Mutex<JobScheduler>>,
    http_client: Arc<ClientWithMiddleware>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // init log
    let timer = OffsetTime::new(offset!(+8), time::format_description::well_known::Rfc3339);
    tracing_subscriber::fmt().with_timer(timer).init();

    // parse cmdline args
    let cli = Cli::parse();

    // new nacos naming sevice
    let naming_service = Arc::new(
        NamingServiceBuilder::new(
            ClientProps::new()
                .server_addr(cli.nacos_addr)
                .naming_push_empty_protection(false),
        )
        .build()?,
    );

    // new http client
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

    // create scheduler
    let mut js = JobScheduler::new(client.clone()).await;
    read_jobs(&cli.job_conf_path)
        .await?
        .into_iter()
        .for_each(|j| js.add(j));
    js.start().await?;

    // 启动http服务
    // build our application with a single route
    let app_state = AppState {
        path: cli.job_conf_path.clone(),
        http_client: client.clone(),
        js: Arc::new(Mutex::new(js)),
    };
    let app = Router::new()
        // .route("/reload", post(reload))
        .route("/trigger/:job_id", post(trigger))
        .with_state(app_state);
    let listener = TcpListener::bind(format!("0.0.0.0:{}", cli.listen_port)).await?;

    let axum_handle = tokio::spawn(async move { axum::serve(listener, app).await });

    // register scheduler self
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

    // wait exit signal
    tokio::select! {
        _ = &mut Box::pin(tokio::signal::ctrl_c()) =>  {
            info!("receive ctrl-c");
        }
        res = &mut Box::pin(axum_handle) => {
            warn!("axum exit, reason: {:?}", res);
        }
    }

    // deregister scheduler self
    naming_service
        .deregister_instance("hlj".to_string(), Some(cli.nacos_group), instance)
        .await?;

    Ok(())
}

async fn read_jobs(path: &PathBuf) -> anyhow::Result<Vec<Job>> {
    let file_content = std::fs::read_to_string(path)?;
    Ok(serde_yaml::from_str::<Jobs>(&file_content)?.jobs)
}

async fn trigger(
    Path(job_id): Path<String>,
    State(state): State<AppState>,
) -> Result<(), AppError> {
    let js = state.js.lock().await;
    js.trigger(&job_id).await?;
    Ok(())
}

// Make our own error that wraps `anyhow::Error`.
struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("err: {}", self.0),
        )
            .into_response()
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
