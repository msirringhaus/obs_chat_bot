use matrix_bot_api::handlers::HandleResult::{ContinueHandling, StopHandling};
use matrix_bot_api::handlers::{HandleResult, StatelessHandler};
use matrix_bot_api::{ActiveBot, MatrixBot, Message, MessageType};

pub fn shutdown(bot: &ActiveBot, _message: &Message, _cmd: &str) -> HandleResult {
    bot.shutdown();
    ContinueHandling
}

pub fn leave(bot: &ActiveBot, message: &Message, _cmd: &str) -> HandleResult {
    bot.send_message("Bye!", &message.room, MessageType::RoomNotice);
    bot.leave_room(&message.room);
    StopHandling
}

pub fn register_handler(bot: &mut MatrixBot, prefix: &Option<&str>) {
    let mut handler = StatelessHandler::new();
    match prefix {
        Some(x) => handler.set_cmd_prefix(x),
        None => { /* Nothing */ }
    }

    handler.register_handle("leave", leave);
    handler.register_handle("shutdown", shutdown);
    bot.add_handler(handler);
}

pub fn help_str(prefix: Option<&str>) -> String {
    format!(
        "{0}leave     - Leave the current room\n{0}shutdown   - Shutdown the bot completely",
        prefix.unwrap_or("")
    )
    .to_string()
}
