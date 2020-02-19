mod build_res;
mod common;
mod leave;
mod submitrequests;

use anyhow::Result;
use common::ConnectionDetails;
use config;

use matrix_bot_api::handlers::{extract_command, HandleResult, MessageHandler};
use matrix_bot_api::{ActiveBot, MatrixBot, Message, MessageType};

use lapin::{Connection, ConnectionProperties};

#[derive(Debug)]
struct HelpHandler {
    prefix: Option<String>,
}

impl MessageHandler for HelpHandler {
    fn handle_message(&mut self, bot: &ActiveBot, message: &Message) -> HandleResult {
        let command = match extract_command(&message.body, self.prefix.as_deref().unwrap_or("")) {
            Some(x) => x,
            None => return HandleResult::ContinueHandling,
        };
        if command != "help" {
            return HandleResult::ContinueHandling;
        }

        let mut msg = "Hi, I'm a friendly robot and provide these options:".to_string();
        msg += "\n";
        msg += self.prefix.as_deref().unwrap_or("");
        msg += "help         - Print this help";
        msg += "\n";
        msg += &leave::help_str(self.prefix.as_deref());
        msg += "\n";
        msg += &build_res::help_str(self.prefix.as_deref());
        msg += "\n";
        msg += &submitrequests::help_str(self.prefix.as_deref());

        bot.send_message(&msg, &message.room, MessageType::RoomNotice);
        return HandleResult::StopHandling;
    }
}

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
    // =========================================================

    // Defining Prefix - default: "!"
    let prefix = settings.get_str("prefix").ok(); // No special prefix at the moment. Replace by Some("myprefix")

    // Defining the first handler for general help
    let help_handler = HelpHandler {
        prefix: prefix.clone(),
    };

    // Creating the bot
    let mut bot = MatrixBot::new(help_handler);

    for details in [OPENSUSE_CONNECTION, SUSE_CONNECTION].iter() {
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
