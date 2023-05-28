use actix_web::middleware::Logger;
use actix_web::{get, App, HttpServer};
use actix_web::{HttpResponse, Responder};
mod config;
mod google_oauth;
mod handler;
mod model;
mod response;
use config::Config;
use google_oauth::*;
use handler::*;
use model::*;
use response::*;

#[get("/api/healthcheck")]
async fn health_check_handler() -> impl Responder {
    const MESSAGE: &str = "How to implemenet Google OAuth2 in Rust";

    HttpResponse::Ok().json(serde_json::json!({"status": "success", "message": MESSAGE}))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "actix_web=info")
    }
    env_logger::init();
    println!("Server successfully started");

    HttpServer::new(move || {
        App::new()
            .service(health_check_handler)
            .wrap(Logger::default())
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
