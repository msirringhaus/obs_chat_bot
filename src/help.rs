use crate::build_res;
use crate::leave;
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
        HandleResult::StopHandling
    }
}
