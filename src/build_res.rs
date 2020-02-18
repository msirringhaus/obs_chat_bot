use crate::common::ConnectionDetails;
use anyhow::{anyhow, Result};
use lapin::{
    message::{Delivery, DeliveryResult},
    options::*,
    types::FieldTable,
    Channel, ConsumerDelegate, ExchangeKind,
};
use matrix_bot_api::handlers::{HandleResult, MessageHandler};
use matrix_bot_api::{ActiveBot, MatrixBot, Message, MessageType};
use serde::Deserialize;
use serde_json;
use std::collections::hash_map::HashMap;
use std::sync::{Arc, Mutex};

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

#[derive(Clone)]
struct Subscriber {
    server_details: ConnectionDetails,
    channel: Channel,
    bot: Arc<Mutex<ActiveBot>>,
    subscriptions: Arc<Mutex<HashMap<(String, String), Vec<String>>>>,
}

impl MessageHandler for Subscriber {
    /// Will be called for every text message send to a room the bot is in
    fn handle_message(&mut self, bot: &ActiveBot, message: &Message) -> HandleResult {
        // Check if its for me
        if !message.body.contains(&self.server_details.domain) {
            return HandleResult::ContinueHandling;
        }

        let parts: Vec<_> = message.body.split("/").collect();
        if parts.len() < 2 {
            println!("Message not parsable");
            bot.send_message(
                "Sorry, I could not parse that. Usage: PROJECT/PACKAGE",
                &message.room,
                MessageType::TextMessage,
            );
            return HandleResult::ContinueHandling;
        }
        let mut iter = parts.iter().rev();
        // These unwraps cannot fail, as there have to be at least 2 parts
        let package = iter.next().unwrap().trim().to_string();
        let project = iter.next().unwrap().trim().to_string();

        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            let key = (project.clone(), package.clone());
            if !subscriptions.contains_key(&key) {
                subscriptions.insert(key.clone(), Vec::new());
            }
            subscriptions
                .get_mut(&key)
                .unwrap() // We know its in there, we just added it above
                .push(message.room.to_string());
            println!(
                "Subscribing room {} to {:?} on {}",
                message.room, key, &self.server_details.domain
            );
        } else {
            println!("subscriptions not lockable");
        }
        HandleResult::ContinueHandling
    }
}

impl Subscriber {
    fn delivery_wrapper(&self, delivery: Delivery) -> Result<()> {
        let data = std::str::from_utf8(&delivery.data)?;
        let jsondata: BuildSuccess = serde_json::from_str(data)?;

        let build_res;
        if delivery.routing_key.as_str().contains("build_success") {
            build_res = "success";
        } else if delivery.routing_key.as_str().contains("build_fail") {
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

impl ConsumerDelegate for Subscriber {
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
    channel
        .exchange_declare(
            "pubsub",
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                passive: true,
                durable: true,
                auto_delete: true, // deactivate me to survive bot reboots
                internal: false,
                nowait: false,
            },
            FieldTable::default(),
        )
        .wait()?;

    let queue = channel
        .queue_declare("", QueueDeclareOptions::default(), FieldTable::default())
        .wait()?;

    for key in [KEY_BUILD_SUCCESS, KEY_BUILD_FAIL].iter() {
        channel
            .queue_bind(
                &queue.name().to_string(),
                "pubsub",
                &format!("{}.{}", details.rabbitscope, key),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .wait()?;
    }

    let consumer = channel
        .basic_consume(
            &queue,
            "OBS_bot_consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .wait()?;

    println!("Subscribing to {}", details.domain);

    let sub = Subscriber {
        server_details: details.clone(),
        channel: channel,
        bot: Arc::new(Mutex::new(bot.get_activebot_clone())),
        subscriptions: Arc::new(Mutex::new(HashMap::new())),
    };
    bot.add_handler(sub.clone());
    consumer.set_delegate(Box::new(sub));

    Ok(())
}
