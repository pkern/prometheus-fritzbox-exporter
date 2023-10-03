use log::warn;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use pomfritz::*;
use rocket::outcome::{try_outcome, Outcome};
use rocket::request::{self, FromRequest, Request};
use rocket::State;
use std::env;
use std::fs;
use std::mem::drop;
use std::path::PathBuf;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

#[macro_use]
extern crate rocket;

struct MyState {
    prometheus_handle: PrometheusHandle,
    session: RwLock<FritzboxSession>,
    config: FritzboxConfig,
}

struct UpdateSession;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for UpdateSession {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, ()> {
        let my_state = try_outcome!(req.guard::<&State<MyState>>().await);
        let current_session = my_state.session.read().await;
        if current_session.still_valid() {
            return Outcome::Success(UpdateSession);
        }
        let new_session = login(&my_state.config, Some(&current_session))
            .await
            .unwrap();
        drop(current_session);
        let mut writable_session = my_state.session.write().await;
        *writable_session = new_session;
        Outcome::Success(UpdateSession)
    }
}

async fn get_data_from_inferior() -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let res = client.get("http://localhost:39714/metrics").send().await?;
    assert_eq!(res.status(), 200);
    let content = res.text().await?;
    Ok(content)
}

#[get("/metrics")]
async fn handle(state: &State<MyState>, _a: UpdateSession) -> String {
    let inferior_data = get_data_from_inferior();
    let session = state.session.read().await;
    fetch_data(&session).await;
    let my_data = state.prometheus_handle.render();
    let result = inferior_data.await;
    match result {
        Ok(data) => format!("{}{}", my_data, data),
        Err(err) => {
            error!("Failed to fetch from Python-based exporter: {}", err);
            my_data
        }
    }
}

#[launch]
async fn rocket() -> _ {
    env_logger::init();

    let contents = fs::read_to_string("config.toml").expect("Could not read configuration file");
    let config: FritzboxConfig =
        toml::from_str(&contents).expect("Could not parse configuration file");

    // Spawn Python-based exporter
    tokio::spawn(async move {
        loop {
            let mut path = match env::current_exe() {
                Ok(mut exe_path) => {
                    exe_path.pop();
                    exe_path.push("fritzbox_exporter.py");
                    exe_path
                }
                Err(e) => {
                    error!("Could not get current exe path: {e}");
                    PathBuf::from(r"")
                }
            };
            // Walk the directory tree to find the Python exporter's binary.
            while !path.exists() {
                path.pop();
                path.pop();
                path.push("fritzbox_exporter.py");
            }

            let mut child = Command::new(path)
                .arg("--verbose")
                .arg("--listen=:39714")
                .arg("--service_skiplist=WANDSLInterfaceConfig1,DeviceConfig1,X_AVM-DE_OnTel1,X_AVM-DE_Filelinks1,WANIPConnection1,WANDSLLinkConfig1,WANPPPConnection1,WANEthernetLinkConfig1")
                .spawn()
                .expect("Failed to spawn Python-based exporter");
            let status = child.wait().await.expect("Failed to wait() on process");
            warn!("Python-based exporter exited with: {}", status);
            sleep(Duration::from_secs(3)).await;
        }
    });

    let session = login(&config, None)
        .await
        .expect("Could not log into Fritzbox");

    let prometheus_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Could not build Prometheus recorder");
    rocket::build()
        .configure(rocket::Config::figment().merge(("port", 9714)))
        .mount("/", routes![handle])
        .manage(MyState {
            prometheus_handle: prometheus_handle,
            session: RwLock::new(session),
            config: config,
        })
}
