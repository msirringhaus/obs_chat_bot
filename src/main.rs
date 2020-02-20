mod build_res;
mod common;
mod help;
mod leave;
mod submitrequests;

use anyhow::Result;
use common::ConnectionDetails;
use config;
use help::HelpHandler;

use matrix_bot_api::MatrixBot;

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
    // ================== Loading credentials ==================
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name("botconfig"))?;

    let user = settings.get_str("user")?;
    let password = settings.get_str("password")?;
    let homeserver_url = settings.get_str("homeserver_url")?;

    let backends = settings.get::<Vec<String>>("backends")?;
    // =========================================================
    // double-check backends
    for backend in &backends {
        if !SUPPORTED_BACKENDS.contains(&backend.as_str()) {
            panic!("Backend {} is not supported!", backend);
        }
    }

    // Defining Prefix - default: "!"
    let prefix = settings.get_str("prefix").ok(); // No special prefix at the moment. Replace by Some("myprefix")

    // Defining the first handler for general help
    let help_handler = HelpHandler {
        prefix: prefix.clone(),
    };

    // Creating the bot
    let mut bot = MatrixBot::new(help_handler);

    for details in [OPENSUSE_CONNECTION, SUSE_CONNECTION].iter() {
        if !backends.contains(&details.domain.to_string()) {
            continue;
        }

        let addr = format!(
            "amqps://{login}@{prefix}.{domain}/%2f",
            login = details.login,
            prefix = details.rabbitprefix,
            domain = details.domain
        );

        let conn = Connection::connect(&addr, ConnectionProperties::default()).wait()?;

        println!("CONNECTED TO {}", &addr);

        let channel = conn.create_channel().wait()?;
        build_res::subscribe(&mut bot, details, channel, prefix.clone())?;

        let channel = conn.create_channel().wait()?;
        submitrequests::subscribe(&mut bot, details, channel, prefix.clone())?;
    }

    leave::register_handler(&mut bot, prefix.as_deref());

    bot.run(&user, &password, &homeserver_url);

    Ok(())
}
