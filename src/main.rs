mod anki;
mod backup;
mod cli;
mod client;
mod commands;
mod config;
mod deck_package;
mod errors;
mod help_catalog;
mod openapi_drift;
mod output;

#[tokio::main]
async fn main() {
    let code = match commands::run().await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            10
        }
    };
    std::process::exit(code);
}
