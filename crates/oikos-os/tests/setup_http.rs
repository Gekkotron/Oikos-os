//! The setup API, served on an ephemeral port over a temp data directory. Brokers and
//! geocoding are closed local ports or local stubs: nothing leaves the machine.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::routing::get;
use oikos_os::config::MqttConfig;
use oikos_os::http::{router_with, setup_router, setup_router_with};
use oikos_os::hub::Hub;
use oikos_os::setup::sections::Sections;
use oikos_os::setup::{HubFile, Mode, RestartHook, SetupCtx};
use oikos_os::weather::open_meteo::Endpoints;
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;

struct Server {
    addr: SocketAddr,
    dir: tempfile::TempDir,
    ctx: Arc<SetupCtx>,
    restarts: Arc<AtomicUsize>,
    client: reqwest::Client,
}

impl Server {
    fn url(&self, path: &str) -> String {
        format!("http://{}/api/v1/setup{path}", self.addr)
    }
    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let r = self.client.get(self.url(path)).send().await.unwrap();
        (r.status(), r.json().await.unwrap_or(Value::Null))
    }
    async fn send(&self, method: reqwest::Method, path: &str, body: Value) -> (StatusCode, Value) {
        let r = self
            .client
            .request(method, self.url(path))
            .json(&body)
            .send()
            .await
            .unwrap();
        (r.status(), r.json().await.unwrap_or(Value::Null))
    }
    async fn put(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.send(reqwest::Method::PUT, path, body).await
    }
    async fn post(&self, path: &str, body: Value) -> (StatusCode, Value) {
        self.send(reqwest::Method::POST, path, body).await
    }
    fn hub_json(&self) -> Option<Vec<u8>> {
        std::fs::read(self.dir.path().join("hub.json")).ok()
    }
}

fn ctx_for(dir: &Path, mode: Mode, file: HubFile) -> (Arc<SetupCtx>, Arc<AtomicUsize>) {
    let restarts = Arc::new(AtomicUsize::new(0));
    let ctx = Arc::new(SetupCtx::new(
        dir.to_path_buf(),
        mode,
        file,
        None,
        Sections::default(),
        RestartHook::Record(Arc::clone(&restarts)),
    ));
    (ctx, restarts)
}

async fn serve(mode: Mode, file: HubFile, geocoding: Option<Endpoints>, with_hub: bool) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, restarts) = ctx_for(dir.path(), mode, file);
    let geo = geocoding.unwrap_or_default();
    let app = if with_hub {
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        let hub = Hub::new(&oikos_os::config::Config::default(), tx);
        router_with(hub, Arc::clone(&ctx), geo)
    } else {
        setup_router_with(Arc::clone(&ctx), geo)
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    Server {
        addr,
        dir,
        ctx,
        restarts,
        client: reqwest::Client::new(),
    }
}

async fn setup_server() -> Server {
    serve(Mode::Setup, HubFile::default(), None, false).await
}

/// A local port nothing listens on.
async fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    l.local_addr().unwrap().port()
}

fn mqtt_file(host: &str) -> HubFile {
    HubFile {
        mqtt: Some(MqttConfig {
            host: host.into(),
            port: 1,
            user: Some("u".into()),
            password: Some("stored-pass".into()),
            ..MqttConfig::default()
        }),
        ..HubFile::default()
    }
}

fn write_bundled(dir: &Path) {
    std::fs::write(
        dir.join("bundled.json"),
        json!({
            "mqtt_user": "oikos", "mqtt_password": "bundled-mqtt-secret",
            "db_user": "oikos", "db_password": "bundled-db-secret", "db_name": "oikos"
        })
        .to_string(),
    )
    .unwrap();
}

#[tokio::test]
async fn status_in_empty_dir() {
    let s = setup_server().await;
    let (code, body) = s.get("").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["mode"], "setup");
    assert_eq!(body["step"], "welcome");
    assert_eq!(body["bundled"], false);
    assert_eq!(body["finished"], false);
    assert!(body["mqtt"].is_null() && body["database"].is_null());
}

#[tokio::test]
async fn health_and_unknown_api_in_setup_mode() {
    let s = setup_server().await;
    let r = reqwest::get(format!("http://{}/health", s.addr))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = reqwest::get(format!("http://{}/api/v1/nope", s.addr))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn bundled_prefills_views_without_leaking() {
    let s = setup_server().await;
    write_bundled(s.dir.path());
    let r = s.client.get(s.url("")).send().await.unwrap();
    let text = r.text().await.unwrap();
    assert!(!text.contains("bundled-mqtt-secret") && !text.contains("bundled-db-secret"));
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["bundled"], true);
    assert_eq!(body["mqtt"]["user"], "oikos");
    assert_eq!(body["mqtt"]["password"], json!({"set": true}));
    assert_eq!(body["database"]["database"], "oikos");
    assert_eq!(body["database"]["password"], json!({"set": true}));
}

#[tokio::test]
async fn stored_passwords_are_never_returned() {
    let mut file = mqtt_file("192.0.2.1");
    file.database_url = Some("postgres://dbu:db-secret@192.0.2.2:5433/oikos".into());
    let s = serve(Mode::Run, file, None, true).await;
    let r = s.client.get(s.url("")).send().await.unwrap();
    let text = r.text().await.unwrap();
    assert!(
        !text.contains("stored-pass") && !text.contains("db-secret"),
        "{text}"
    );
    let body: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(body["mode"], "running");
    assert_eq!(body["mqtt"]["password"], json!({"set": true}));
    assert_eq!(body["database"]["host"], "192.0.2.2");
    assert_eq!(body["database"]["port"], 5433);
    assert_eq!(body["database"]["user"], "dbu");
}

fn paris() -> Value {
    json!({"city":"Paris","country":"FR","latitude":48.85,"longitude":2.35,"timezone":"Europe/Paris"})
}

#[tokio::test]
async fn location_put_advances_to_mqtt() {
    let s = setup_server().await;
    let (code, _) = s.put("/location", paris()).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(s.get("").await.1["step"], "mqtt");
    assert_eq!(s.get("").await.1["location"]["city"], "Paris");
    let file = HubFile::load(s.dir.path()).unwrap().unwrap();
    assert_eq!(file.location.unwrap().city, "Paris");
    assert_eq!(
        s.restarts.load(Ordering::SeqCst),
        0,
        "setup mode does not restart"
    );
}

#[tokio::test]
async fn location_put_clears_the_imported_fallbacks() {
    let file = HubFile {
        pending_city: Some(("Lyon".into(), None)),
        timezone: Some("Europe/Paris".into()),
        ..HubFile::default()
    };
    let s = serve(Mode::Setup, file, None, false).await;
    assert_eq!(s.put("/location", paris()).await.0, StatusCode::OK);
    let saved = HubFile::load(s.dir.path()).unwrap().unwrap();
    assert!(saved.pending_city.is_none() && saved.timezone.is_none());
}

#[tokio::test]
async fn location_with_a_bad_timezone_is_422() {
    let s = setup_server().await;
    let mut bad = paris();
    bad["timezone"] = json!("Mars/Olympus");
    assert_eq!(
        s.put("/location", bad).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn location_in_run_mode_restarts_once() {
    let s = serve(Mode::Run, HubFile::default(), None, true).await;
    assert_eq!(s.put("/location", paris()).await.0, StatusCode::OK);
    assert_eq!(s.restarts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_mqtt_probe_saves_nothing() {
    let s = setup_server().await;
    let before = s.hub_json();
    let port = closed_port().await;
    let (code, body) = s
        .put(
            "/mqtt",
            json!({"host":"127.0.0.1","port":port,"user":"u","password":"pw"}),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["ok"], false);
    assert_eq!(s.hub_json(), before);
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn failed_mqtt_probe_on_a_running_hub_leaves_the_file_and_does_not_restart() {
    let s = serve(Mode::Run, mqtt_file("192.0.2.1"), None, true).await;
    s.ctx.update_file(|_| {}).await.unwrap();
    let before = s.hub_json().unwrap();
    let (code, body) = s
        .put(
            "/mqtt",
            json!({"host":"127.0.0.1","port":1,"user":"u","password":"wrong"}),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["ok"], false);
    assert_eq!(s.hub_json().unwrap(), before);
    assert_eq!(s.restarts.load(Ordering::SeqCst), 0);
    // Nothing reached the 422 body either.
    assert!(!body.to_string().contains("wrong"));
}

#[tokio::test]
async fn mqtt_test_reports_without_saving() {
    let s = setup_server().await;
    let port = closed_port().await;
    let (code, body) = s
        .post("/test/mqtt", json!({"host":"127.0.0.1","port":port}))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["ok"], false);
    assert!(body["message"].is_string());
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn mqtt_new_host_without_password_is_422() {
    let s = serve(Mode::Setup, mqtt_file("a.example.com"), None, false).await;
    for path in ["/mqtt", "/test/mqtt"] {
        let (code, body) = s
            .send(
                if path == "/mqtt" {
                    reqwest::Method::PUT
                } else {
                    reqwest::Method::POST
                },
                path,
                json!({"host":"b.example.com","port":1883}),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(body["error"], "Enter the password for the new address");
    }
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn bundled_password_does_not_follow_a_new_host() {
    let s = setup_server().await;
    write_bundled(s.dir.path());
    let (code, body) = s
        .post(
            "/test/mqtt",
            json!({"host":"attacker.example.com","port":1883}),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "Enter the password for the new address");
}

#[tokio::test]
async fn database_new_host_without_password_is_422() {
    let file = HubFile {
        database_url: Some("postgres://u:p@a.example.com:5432/oikos".into()),
        ..HubFile::default()
    };
    let s = serve(Mode::Setup, file, None, false).await;
    let (code, body) = s
        .put(
            "/database",
            json!({"host":"b.example.com","port":5432,"database":"oikos","user":"u"}),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"], "Enter the password for the new address");
}

#[tokio::test]
async fn failed_database_probe_saves_nothing() {
    let s = setup_server().await;
    let port = closed_port().await;
    let (code, body) = s
        .put(
            "/database",
            json!({"host":"127.0.0.1","port":port,"database":"oikos","user":"u","password":"pw-secret-9"}),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["ok"], false);
    assert!(!body.to_string().contains("pw-secret-9"));
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn step_put_never_finishes() {
    let s = setup_server().await;
    let (code, body) = s.put("/step", json!({"step":"done"})).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["step"], "done");
    assert_eq!(body["finished"], false);
    assert!(!HubFile::load(s.dir.path()).unwrap().unwrap().finished);
}

#[tokio::test]
async fn finish_sets_finished_and_done() {
    let s = setup_server().await;
    let (code, body) = s.post("/finish", json!({})).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["finished"], true);
    assert_eq!(body["step"], "done");
}

#[tokio::test]
async fn second_write_during_a_pending_restart_is_503() {
    let s = serve(Mode::Run, HubFile::default(), None, true).await;
    assert_eq!(s.put("/location", paris()).await.0, StatusCode::OK);
    let after_first = s.hub_json().unwrap();
    for (method, path, body) in [
        (reqwest::Method::PUT, "/location", paris()),
        (reqwest::Method::PUT, "/step", json!({"step":"mqtt"})),
        (reqwest::Method::POST, "/finish", json!({})),
        (
            reqwest::Method::PUT,
            "/mqtt",
            json!({"host":"127.0.0.1","port":1}),
        ),
    ] {
        let (code, body) = s.send(method, path, body).await;
        assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{path}");
        assert_eq!(body, json!({"error":"restarting"}));
    }
    assert_eq!(s.hub_json().unwrap(), after_first);
    assert_eq!(s.restarts.load(Ordering::SeqCst), 1);
    assert!(HubFile::load(s.dir.path()).is_ok());
}

#[tokio::test]
async fn geocode_returns_at_most_five_places_without_secrets() {
    let results: Vec<Value> = (0..8)
        .map(|i| {
            json!({"name": format!("Paris {i}"), "admin1": "Ile", "country_code": "FR",
                   "latitude": 48.0 + f64::from(i), "longitude": 2.0, "timezone": "Europe/Paris"})
        })
        .collect();
    let body = json!({ "results": results });
    let app = Router::new().route(
        "/geocode",
        get(move || {
            let body = body.clone();
            async move { axum::Json(body) }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, app).into_future());
    let ep = Endpoints {
        geocoding: format!("http://{base}/geocode"),
        forecast: "http://127.0.0.1:1/unused".into(),
    };
    let s = serve(Mode::Setup, HubFile::default(), Some(ep), false).await;
    let (code, body) = s.get("/geocode?city=Paris&country=FR").await;
    assert_eq!(code, StatusCode::OK);
    let places = body.as_array().unwrap();
    assert_eq!(places.len(), 5);
    assert_eq!(places[0]["name"], "Paris 0");
    assert_eq!(places[0]["timezone"], "Europe/Paris");
    assert_eq!(s.get("/geocode?city=").await.1, json!([]));
}

#[tokio::test]
async fn geocode_failure_is_502() {
    let ep = Endpoints {
        geocoding: format!("http://127.0.0.1:{}/geocode", closed_port().await),
        forecast: String::new(),
    };
    let s = serve(Mode::Setup, HubFile::default(), Some(ep), false).await;
    assert_eq!(
        s.get("/geocode?city=Paris").await.0,
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn setup_router_default_builds() {
    let (ctx, _) = ctx_for(
        tempfile::tempdir().unwrap().path(),
        Mode::Setup,
        HubFile::default(),
    );
    let _ = setup_router(ctx);
}

/// A one-shot broker: reads the CONNECT, answers a CONNACK with `code`, keeps the socket open.
async fn fake_broker(code: u8) -> u16 {
    fake_broker_with(code, None).await
}

/// Like [`fake_broker`]; `flag` is set the moment a CONNECT arrives, before the answer.
async fn fake_broker_with(code: u8, flag: Option<Arc<std::sync::atomic::AtomicBool>>) -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let flag = flag.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 512];
                let _ = sock.read(&mut buf).await;
                if let Some(flag) = flag {
                    flag.store(true, Ordering::SeqCst);
                }
                let _ = sock.write_all(&[0x20, 0x02, 0x00, code]).await;
                let _ = sock.read(&mut buf).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn mqtt_put_saves_after_a_good_probe() {
    let port = fake_broker(0).await;
    let s = setup_server().await;
    let (code, body) = s
        .put(
            "/mqtt",
            json!({"host":"127.0.0.1","port":port,"user":"u","password":"pw","z2m_base":"z2m"}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["step"], "database");
    assert_eq!(body["mqtt"]["password"], json!({"set": true}));
    assert!(!body.to_string().contains("\"pw\""));
    let file = HubFile::load(s.dir.path()).unwrap().unwrap();
    assert_eq!(file.mqtt.unwrap().password.as_deref(), Some("pw"));
    assert_eq!(file.z2m_base.as_deref(), Some("z2m"));
    assert_eq!(s.restarts.load(Ordering::SeqCst), 0, "not complete yet");
}

#[tokio::test]
async fn mqtt_put_that_completes_the_file_restarts() {
    let port = fake_broker(0).await;
    let file = HubFile {
        database_url: Some("postgres://u:p@192.0.2.2/oikos".into()),
        ..HubFile::default()
    };
    let s = serve(Mode::Setup, file, None, false).await;
    let (code, _) = s
        .put("/mqtt", json!({"host":"127.0.0.1","port":port}))
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(s.restarts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn mqtt_bad_credentials_are_reported() {
    let port = fake_broker(4).await;
    let s = setup_server().await;
    let (code, body) = s
        .post(
            "/test/mqtt",
            json!({"host":"127.0.0.1","port":port,"user":"u","password":"x"}),
        )
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["ok"], false);
    assert_eq!(body["message"], "bad credentials");
    let port = fake_broker(0).await;
    let (_, body) = s
        .post("/test/mqtt", json!({"host":"127.0.0.1","port":port}))
        .await;
    assert_eq!(body, json!({"ok":true,"message":"connected","detail":null}));
}

#[tokio::test]
async fn bad_broker_credentials_on_a_running_hub_are_never_saved() {
    let port = fake_broker(4).await;
    let s = serve(Mode::Run, mqtt_file("192.0.2.1"), None, true).await;
    s.ctx.update_file(|_| {}).await.unwrap();
    let before = s.hub_json().unwrap();
    let (code, body) = s
        .put(
            "/mqtt",
            json!({"host":"127.0.0.1","port":port,"user":"u","password":"typo-pass"}),
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["ok"], false);
    assert_eq!(body["message"], "bad credentials");
    assert!(!body.to_string().contains("typo-pass"));
    assert_eq!(s.hub_json().unwrap(), before);
    assert_eq!(s.restarts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_restart_scheduled_during_the_probe_refuses_the_save() {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let port = fake_broker_with(0, Some(Arc::clone(&flag))).await;
    let s = serve(Mode::Setup, HubFile::default(), None, false).await;
    // The broker raises `restarting` (as a racing write would) after the gate, during the probe.
    let ctx_flag = Arc::clone(&s.ctx);
    let watcher = tokio::spawn(async move {
        while !flag.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
        ctx_flag.restarting.store(true, Ordering::SeqCst);
    });
    let (code, body) = s
        .put("/mqtt", json!({"host":"127.0.0.1","port":port}))
        .await;
    watcher.await.unwrap();
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body, json!({"error":"restarting"}));
    assert!(s.hub_json().is_none(), "nothing saved");
}

#[tokio::test]
async fn password_without_a_user_is_refused() {
    let s = setup_server().await;
    let port = closed_port().await;
    for path in ["/mqtt", "/test/mqtt"] {
        let (code, body) = s
            .send(
                if path == "/mqtt" {
                    reqwest::Method::PUT
                } else {
                    reqwest::Method::POST
                },
                path,
                json!({"host":"127.0.0.1","port":port,"password":"pw"}),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(body["error"], "Enter a user name for the password");
    }
    assert!(s.hub_json().is_none());
}

const NEW_ADDRESS: &str = "Enter the password for the new address";

#[tokio::test]
async fn mqtt_same_host_other_port_needs_the_password() {
    let s = serve(Mode::Setup, mqtt_file("127.0.0.1"), None, false).await; // stored port 1
    for (method, path) in [
        (reqwest::Method::PUT, "/mqtt"),
        (reqwest::Method::POST, "/test/mqtt"),
    ] {
        let (code, body) = s
            .send(
                method,
                path,
                json!({"host":"127.0.0.1","port":2,"user":"u"}),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(body["error"], NEW_ADDRESS);
    }
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn bundled_mqtt_password_stays_on_the_bundled_port() {
    let s = setup_server().await;
    write_bundled(s.dir.path());
    for (method, path) in [
        (reqwest::Method::PUT, "/mqtt"),
        (reqwest::Method::POST, "/test/mqtt"),
    ] {
        let (code, body) = s
            .send(
                method,
                path,
                json!({"host":"localhost","port":1884,"user":"oikos"}),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(body["error"], NEW_ADDRESS);
    }
    assert!(s.hub_json().is_none());
}

#[tokio::test]
async fn database_same_host_other_port_needs_the_password() {
    let file = HubFile {
        database_url: Some("postgres://u:p@127.0.0.1:5432/oikos".into()),
        ..HubFile::default()
    };
    let s = serve(Mode::Setup, file, None, false).await;
    for (method, path) in [
        (reqwest::Method::PUT, "/database"),
        (reqwest::Method::POST, "/test/database"),
    ] {
        let (code, body) = s
            .send(
                method,
                path,
                json!({"host":"127.0.0.1","port":5433,"database":"oikos","user":"u"}),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(body["error"], NEW_ADDRESS);
    }
}

#[tokio::test]
async fn bundled_database_password_stays_on_the_bundled_port() {
    let s = setup_server().await;
    write_bundled(s.dir.path());
    for (method, path) in [
        (reqwest::Method::PUT, "/database"),
        (reqwest::Method::POST, "/test/database"),
    ] {
        let (code, body) = s
            .send(
                method,
                path,
                json!({"host":"localhost","port":5433,"database":"oikos","user":"oikos"}),
            )
            .await;
        assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
        assert_eq!(body["error"], NEW_ADDRESS);
    }
}

#[tokio::test]
async fn database_name_or_user_change_alone_is_not_a_new_address() {
    let port = closed_port().await;
    let file = HubFile {
        database_url: Some(format!("postgres://u:p@127.0.0.1:{port}/oikos")),
        ..HubFile::default()
    };
    let s = serve(Mode::Setup, file, None, false).await;
    let (code, body) = s
        .post(
            "/test/database",
            json!({"host":"127.0.0.1","port":port,"database":"other","user":"v"}),
        )
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], false, "the probe ran (and found no server)");
}
