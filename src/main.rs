use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use pomfritz::*;
use rocket::outcome::{try_outcome, Outcome};
use rocket::request::{self, FromRequest, Request};
use rocket::State;
use std::fs;
use std::mem::drop;
use tokio::sync::RwLock;

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

#[get("/metrics")]
async fn handle(state: &State<MyState>, _a: UpdateSession) -> String {
    let session = state.session.read().await;
    fetch_data(&session).await;
    state.prometheus_handle.render()
}

#[launch]
async fn rocket() -> _ {
    env_logger::init();

    let contents = fs::read_to_string("config.toml").expect("Could not read configuration file");
    let config: FritzboxConfig =
        toml::from_str(&contents).expect("Could not parse configuration file");

    let session = login(&config, None)
        .await
        .expect("Could not log into Fritzbox");

    let prometheus_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Could not build Prometheus recorder");
    rocket::build().mount("/", routes![handle]).manage(MyState {
        prometheus_handle: prometheus_handle,
        session: RwLock::new(session),
        config: config,
    })
}
