use crate::build_res;
use crate::leave;
use crate::openqa;
use crate::submitrequests;

use matrix_bot_api::handlers::{extract_command, HandleResult, MessageHandler};
use matrix_bot_api::{ActiveBot, Message, MessageType};

#[derive(Debug)]
pub struct HelpHandler {
    pub prefix: Option<String>,
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

        let mut items = vec![("help".to_string(), "Print this help".to_string())];
        items.append(&mut leave::help_str(self.prefix.as_deref()));
        items.append(&mut build_res::help_str(self.prefix.as_deref()));
        items.append(&mut submitrequests::help_str(self.prefix.as_deref()));
        items.append(&mut openqa::help_str(self.prefix.as_deref()));

        let mut plainmsg = "Hi, I'm a friendly robot and provide these options:".to_string();
        for (key, text) in &items {
            plainmsg += "\n";
            plainmsg += &format!("{:<35} - {}", key, text)
        }

        let mut htmlmsg =
            "<h3>Hi, I'm a friendly robot and provide these options:</h3>".to_string();
        htmlmsg += "\n";
        htmlmsg += "<table>";
        for (key, text) in &items {
            htmlmsg += "\n";
            htmlmsg += &format!("<tr> <td>{}</td> <td>{}</td></tr>", key, text)
        }
        htmlmsg += "\n";
        htmlmsg += "</table>";

        bot.send_html_message(&plainmsg, &htmlmsg, &message.room, MessageType::TextMessage);
        HandleResult::StopHandling
    }
}
