use crate::common::{ConnectionDetails, Subscriber};
use anyhow::{anyhow, Result};
use lapin::{
    message::{Delivery, DeliveryResult},
    options::*,
    Channel, ConsumerDelegate,
};
use matrix_bot_api::handlers::{HandleResult, MessageHandler};
use matrix_bot_api::{ActiveBot, MatrixBot, Message, MessageType};
use serde::Deserialize;
use serde_json;

const KEY_BUILD_SUCCESS: &str = "obs.package.build_success";
const KEY_BUILD_FAIL: &str = "obs.package.build_fail";

#[derive(Deserialize, Debug)]
struct BuildSuccess {
    arch: String,
    repository: String,
    package: String,
    project: String,
    reason: Option<String>,
    release: Option<String>,
    readytime: Option<String>,
    srcmd5: Option<String>,
    rev: Option<String>,
    bcnt: Option<String>,
    verifymd5: Option<String>,
    starttime: Option<String>,
    endtime: Option<String>,
    workerid: Option<String>,
    versrel: Option<String>,
    hostarch: Option<String>,
    previouslyfailed: Option<String>,
}

impl MessageHandler for Subscriber<(String, String)> {
    /// Will be called for every text message send to a room the bot is in
    fn handle_message(&mut self, bot: &ActiveBot, message: &Message) -> HandleResult {
        // Check if its for me
        if !message
            .body
            .contains(&format!("{}/package/", self.server_details.domain))
        {
            return HandleResult::ContinueHandling;
        }

        let parts: Vec<_> = message.body.split("/").collect();
        if parts.len() < 3 {
            println!("Message not parsable");
            bot.send_message(
                "Sorry, I could not parse that. Please post a package-URL",
                &message.room,
                MessageType::TextMessage,
            );
            return HandleResult::ContinueHandling;
        }
        let mut iter = parts.iter().rev();
        // These unwraps cannot fail, as there have to be at least 2 parts
        let package = iter.next().unwrap().trim().to_string();
        let project = iter.next().unwrap().trim().to_string();

        let key = (project.clone(), package.clone());
        self.add_to_subscriptions(key, bot, &message.room);
        HandleResult::ContinueHandling
    }
}

impl Subscriber<(String, String)> {
    fn delivery_wrapper(&self, delivery: Delivery) -> Result<()> {
        let data = std::str::from_utf8(&delivery.data)?;
        let jsondata: BuildSuccess = serde_json::from_str(data)?;

        let build_res;
        if delivery.routing_key.as_str().contains(KEY_BUILD_SUCCESS) {
            build_res = "succeeded";
        } else if delivery.routing_key.as_str().contains(KEY_BUILD_FAIL) {
            build_res = "failed";
        } else {
            return Err(anyhow!(
                "Build event neither success nor failure, but {}",
                delivery.routing_key.as_str()
            ));
        }

        let key = (jsondata.project.clone(), jsondata.package.clone());
        let rooms;
        if let Ok(subscriptions) = self.subscriptions.lock() {
            // This is a message we are not subscribed to
            if !subscriptions.contains_key(&key) {
                return Ok(());
            }

            rooms = subscriptions[&key].clone();
        } else {
            return Ok(());
        }

        println!(
            "Build {}: {} {} ({})",
            build_res, jsondata.project, jsondata.package, jsondata.arch
        );

        if let Ok(bot) = self.bot.lock() {
            for room in rooms {
                bot.send_html_message(
                    &format!(
                        "Build {}: {}/{} ({} / {})",
                        build_res,
                        jsondata.project,
                        jsondata.package,
                        jsondata.arch,
                        jsondata.repository,
                    ),
                    &format!(
                        "<strong>Build {}</strong>: <a href={}>{}/{}</a> ({} / {})",
                        if build_res == "success" {
                            build_res.to_string()
                        } else {
                            format!("<u>{}</u>", build_res)
                        },
                        format!(
                            "https://{}.{}/package/show/{}/{}",
                            self.server_details.buildprefix,
                            self.server_details.domain,
                            jsondata.project,
                            jsondata.package,
                        ),
                        jsondata.project,
                        jsondata.package,
                        jsondata.arch,
                        jsondata.repository,
                    ),
                    &room,
                    MessageType::TextMessage,
                );
            }
        }

        self.channel
            .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
            .wait()?;

        Ok(())
    }
}

impl ConsumerDelegate for Subscriber<(String, String)> {
    fn on_new_delivery(&self, delivery: DeliveryResult) {
        if let Ok(Some(delivery)) = delivery {
            match self.delivery_wrapper(delivery) {
                Ok(_) => {}
                Err(x) => println!("Error while getting Event: {:?}. Skipping to continue", x),
            }
        } else {
            println!("Delivery not ok");
        }
    }
}

pub fn subscribe(bot: &mut MatrixBot, details: &ConnectionDetails, channel: Channel) -> Result<()> {
    let subnames = [KEY_BUILD_SUCCESS, KEY_BUILD_FAIL];
    crate::common::subscribe::<(String, String)>(bot, details, channel, &subnames)
}
