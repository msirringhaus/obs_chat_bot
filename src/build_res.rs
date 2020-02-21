use crate::common::{prepend_prefix, ConnectionDetails, Subscriber};
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
use std::collections::hash_map::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

const KEY_BUILD_SUCCESS: &str = "obs.package.build_success";
const KEY_BUILD_FAIL: &str = "obs.package.build_fail";

pub fn help_str(prefix: Option<&str>) -> Vec<(String, String)> {
    let without_prefix = [
        (
            "OBS_PACKAGE_URL",
            "Subscribe to a package. Get notification if build-status changes.",
        ),
        (
            "unsub OBS_PACKAGE_URL",
            "Unsubscribe from a package. Get no more notifications.",
        ),
        (
            "list packages",
            "List all packages currently subscribed to.",
        ),
    ];

    prepend_prefix(prefix, &without_prefix)
}

#[derive(Debug, Clone, std::cmp::PartialEq, std::cmp::Eq, Hash)]
pub struct PackageKey {
    pub project: String,
    pub package: String,
}

impl std::fmt::Display for PackageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.project, self.package)
    }
}

impl TryFrom<String> for PackageKey {
    type Error = ();

    fn try_from(line: String) -> Result<Self, Self::Error> {
        let line = line.trim();
        if line.contains('\n') {
            return Err(());
        }

        let parts: Vec<_> = line.split('/').collect();
        if parts.len() < 4 {
            return Err(());
        }

        let mut iter = parts.iter().rev();
        // These unwraps cannot fail, as there have to be at least 2 parts
        let package = iter.next().unwrap().trim().to_string();
        let project = iter.next().unwrap().trim().to_string();

        Ok(PackageKey { project, package })
    }
}

#[derive(Deserialize, Debug)]
struct BuildSuccessInfo {
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

impl MessageHandler for Subscriber<PackageKey> {
    /// Will be called for every text message send to a room the bot is in
    fn handle_message(&mut self, bot: &ActiveBot, message: &Message) -> HandleResult {
        self.handle_message_helper(bot, &message.body, &message.room);

        HandleResult::ContinueHandling
    }
}

impl Subscriber<PackageKey> {
    fn generate_messages(&self, jsondata: BuildSuccessInfo, changetype: &str) -> (String, String) {
        let plain = format!(
            "Build {}: {}/{} ({} / {})",
            changetype, jsondata.project, jsondata.package, jsondata.arch, jsondata.repository,
        );

        let html = format!(
            "<strong>Build {}</strong>: <a href={}>{}/{}</a> ({} / {})",
            if changetype == "succeeded" {
                changetype.to_string()
            } else {
                format!("<u>{}</u>", changetype)
            },
            format!(
                "{}/{}/{}",
                self.get_base_url(),
                jsondata.project,
                jsondata.package,
            ),
            jsondata.project,
            jsondata.package,
            jsondata.arch,
            jsondata.repository,
        );

        (plain, html)
    }

    fn delivery_wrapper(&self, delivery: Delivery) -> Result<()> {
        let data = std::str::from_utf8(&delivery.data)?;
        let jsondata: BuildSuccessInfo = serde_json::from_str(data)?;

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

        let key = PackageKey {
            project: jsondata.project.clone(),
            package: jsondata.package.clone(),
        };
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
            let (plain, html) = self.generate_messages(jsondata, build_res);
            for room in &rooms {
                bot.send_html_message(&plain, &html, room, MessageType::TextMessage);
            }
        }

        self.channel
            .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
            .wait()?;

        Ok(())
    }
}

impl ConsumerDelegate for Subscriber<PackageKey> {
    fn on_new_delivery(&self, delivery: DeliveryResult) {
        if let Ok(Some(delivery)) = delivery {
            let _ = self
                .channel
                .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                .wait();
            match self.delivery_wrapper(delivery) {
                Ok(_) => {}
                Err(x) => println!("Error while getting Event: {:?}. Skipping to continue", x),
            }
        } else {
            println!("Delivery not ok");
        }
    }
}

pub fn subscribe(
    bot: &mut MatrixBot,
    details: &ConnectionDetails,
    channel: Channel,
    prefix: Option<String>,
    default_subs: &Option<Vec<(String, String)>>,
) -> Result<()> {
    let subnames = [KEY_BUILD_SUCCESS, KEY_BUILD_FAIL];
    let (channel, consumer) = crate::common::subscribe(details, channel, &subnames)?;
    let activebot = bot.get_activebot_clone();
    let mut sub: Subscriber<PackageKey> = Subscriber {
        subtype: "package".to_string(),
        server_details: *details,
        channel,
        bot: Arc::new(Mutex::new(activebot)),
        subscriptions: Arc::new(Mutex::new(HashMap::new())),
        prefix,
    };

    match default_subs {
        None => {}
        Some(subs) => {
            for (room, url) in subs {
                sub.subscribe_to_defaults(&url, &room);
            }
        }
    }

    bot.add_handler(sub.clone());
    consumer.set_delegate(Box::new(sub));

    Ok(())
}
