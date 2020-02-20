mod build_res;
mod common;
mod help;
mod leave;
mod submitrequests;

use anyhow::{anyhow, Result};
use common::ConnectionDetails;
use config;
use help::HelpHandler;
use matrix_bot_api::MatrixBot;
use std::env::args;
use xdg;

use lapin::{Connection, ConnectionProperties};

const SUPPORTED_BACKENDS: [&str; 2] = ["opensuse.org", "suse.de"];

const SUSE_CONNECTION: ConnectionDetails = ConnectionDetails {
    domain: "suse.de",
    login: "suse:suse",
    buildprefix: "build",
    rabbitprefix: "rabbit",
    rabbitscope: "suse",
};

const OPENSUSE_CONNECTION: ConnectionDetails = ConnectionDetails {
    domain: "opensuse.org",
    login: "opensuse:opensuse",
    buildprefix: "build",
    rabbitprefix: "rabbit",
    rabbitscope: "opensuse",
};

fn main() -> Result<()> {
    // ================== Search for config file  ==================
    // If we have a commandline argument, use that. If not, search XDG-paths
    let config_path = match args().nth(1) {
        Some(x) => std::path::PathBuf::from(x),
        None => {
            let dirs = xdg::BaseDirectories::with_prefix("obs_chat_bot")?;
            dirs.find_config_file("config.toml").ok_or_else(|| {
                anyhow!(
                    "No config-file found! Looked for config.toml in your XDG-paths ({:?}, {:?})",
                    dirs.get_config_home(),
                    dirs.get_config_dirs()
                )
            })?
        }
    };

    // ================== Loading credentials ==================
    let mut settings = config::Config::default();
    settings.merge(config::File::from(config_path))?;

    let user = settings.get_str("user")?;
    let password = settings.get_str("password")?;
    let homeserver_url = settings.get_str("homeserver_url")?;

    let backends = settings.get::<Vec<String>>("backends")?;

    let prefix = settings.get_str("prefix").ok();

    let default_subs = settings.get::<Vec<(String, String)>>("default_subs").ok();
    // =========================================================

    // Check if backends are supported
    for backend in &backends {
        if !SUPPORTED_BACKENDS.contains(&backend.as_str()) {
            panic!("Backend {} is not supported!", backend);
        }
    }

    // Defining the first handler for general help output
    let help_handler = HelpHandler {
        prefix: prefix.clone(),
    };

    // Creating the bot
    let mut bot = MatrixBot::new(help_handler);

    // Add another handler to handle leave and shutdown
    leave::register_handler(&mut bot, prefix.as_deref());

    // Establish connections to all chosen backends
    for details in [OPENSUSE_CONNECTION, SUSE_CONNECTION].iter() {
        if !backends.contains(&details.domain.to_string()) {
            continue;
        }

        let addr = format!(
            "amqps://{login}@{prefix}.{domain}/%2f", // don't know why /%2f is needed, but it fails without it
            login = details.login,
            prefix = details.rabbitprefix,
            domain = details.domain
        );

        let conn = Connection::connect(&addr, ConnectionProperties::default()).wait()?;

        println!("CONNECTED TO {}", &addr);

        // Subscribe to build_success/build_fails
        let channel = conn.create_channel().wait()?;
        build_res::subscribe(&mut bot, details, channel, prefix.clone(), &default_subs)?;

        // Subscribe to request-changes
        let channel = conn.create_channel().wait()?;
        submitrequests::subscribe(&mut bot, details, channel, prefix.clone(), &default_subs)?;
    }

    // Blocking call until shutdown is issued
    bot.run(&user, &password, &homeserver_url);

    Ok(())
}
