use pomfritz::fetch_data;
use rocket::State;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

#[macro_use] extern crate rocket;

// TODO:
// - Make the login process an async background task, re-checking if the
//   SID is still valid and otherwise performing another login.

struct MyState {
    prometheus_handle: PrometheusHandle
}

#[get("/metrics")]
async fn handle(state: &State<MyState>) -> String {
    fetch_data().await;
    state.prometheus_handle.render()
}

#[launch]
fn rocket() -> _ {
    let prometheus_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Could not build Prometheus recorder");
    rocket::build()
        .mount("/", routes![handle])
        .manage(MyState { prometheus_handle })
}
