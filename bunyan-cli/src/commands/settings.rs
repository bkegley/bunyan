use clap::Subcommand;
use serde::Serialize;

use bunyan_core::models::Setting;

use crate::client::BunyanClient;
use crate::output::{self, OutputMode};

#[derive(Subcommand)]
pub enum SettingsCommand {
    /// List all settings
    List,
    /// Get a setting by key
    Get {
        /// Setting key
        key: String,
    },
    /// Set a setting value
    Set {
        /// Setting key
        key: String,
        /// Setting value
        value: String,
    },
}

#[derive(Serialize)]
struct SetBody {
    value: String,
}

pub fn run(client: &BunyanClient, cmd: SettingsCommand, mode: OutputMode) {
    match cmd {
        SettingsCommand::List => {
            let settings: Vec<Setting> = client.get("/settings").unwrap_or_else(|e| {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            });
            match mode {
                OutputMode::Quiet => {
                    for s in &settings {
                        println!("{}", s.key);
                    }
                }
                OutputMode::Json => output::print_value(mode, &settings),
                OutputMode::Table => {
                    let rows: Vec<Vec<String>> = settings
                        .iter()
                        .map(|s| vec![s.key.clone(), s.value.clone()])
                        .collect();
                    output::print_table(&["KEY", "VALUE"], &rows);
                }
            }
        }
        SettingsCommand::Get { key } => {
            let setting: Setting =
                client.get(&format!("/settings/{}", key)).unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => println!("{}", setting.value),
                _ => output::print_value(mode, &setting),
            }
        }
        SettingsCommand::Set { key, value } => {
            let body = SetBody { value };
            let setting: Setting = client
                .put(&format!("/settings/{}", key), &body)
                .unwrap_or_else(|e| {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                });
            match mode {
                OutputMode::Quiet => println!("{}", setting.value),
                _ => output::print_value(mode, &setting),
            }
        }
    }
}
