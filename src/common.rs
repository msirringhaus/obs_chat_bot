use anyhow::Result;
use lapin::{options::*, types::FieldTable, Channel, Consumer, ExchangeKind};
use matrix_bot_api::{ActiveBot, MessageType};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy)]
pub struct ConnectionDetails {
    pub domain: &'static str,
    pub login: &'static str,
    pub buildprefix: &'static str,
    pub rabbitprefix: &'static str,
    pub rabbitscope: &'static str,
}

#[derive(Clone)]
pub struct Subscriber<T>
where
    T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Display + TryFrom<String>,
{
    pub server_details: ConnectionDetails,
    pub channel: Channel,
    pub bot: Arc<Mutex<ActiveBot>>,
    pub subscriptions: Arc<Mutex<HashMap<T, HashSet<String>>>>,
    pub prefix: Option<String>,
    pub subtype: String,
}

#[derive(Debug)]
pub enum ScanLineResult {
    NotForMe,
    ListCommand,
    PossiblyForMe,
}

impl<T> Subscriber<T>
where
    T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Display + TryFrom<String>,
{
    pub fn get_base_url(&self) -> String {
        let tail = if self.server_details.buildprefix == "openqa" {
            String::new()
        } else {
            "/show".to_string()
        };

        format!(
            "https://{}.{}/{}{}",
            self.server_details.buildprefix, self.server_details.domain, self.subtype, tail
        )
    }

    pub fn list_keys(&self, bot: &ActiveBot, room: &str) {
        if let Ok(subscriptions) = self.subscriptions.lock() {
            let mut found_subscriptions = Vec::new();

            for (key, value) in subscriptions.iter() {
                if value.contains(room) {
                    found_subscriptions.push(key.clone());
                }
            }

            let (htmlanswer, plainanswer) = if found_subscriptions.is_empty() {
                let answer = "No subscriptions found";
                (answer.to_string(), answer.to_string())
            } else {
                let mut unsorted = found_subscriptions
                    .iter()
                    .map(|x| format!("{}", x))
                    .collect::<Vec<_>>();
                unsorted.sort();

                unsorted = found_subscriptions
                    .iter()
                    .map(|x| format!("<a href={}/{}>{}</a>", self.get_base_url(), x, x))
                    .collect::<Vec<_>>();
                unsorted.sort();

                (unsorted.join("<br>"), unsorted.join(", "))
            };

            let plainanswer = format!("On {}: {}", self.server_details.domain, plainanswer);
            let htmlanswer = format!("On {}:<br>{}", self.server_details.domain, htmlanswer);

            bot.send_html_message(&plainanswer, &htmlanswer, room, MessageType::TextMessage);
        } else {
            println!("ERROR! list_keys: subscriptions not lockable");
            bot.send_message(
                "Sorry, I could not list your requests, due to an internal error.",
                room,
                MessageType::TextMessage,
            );
        }
    }

    pub fn subscribe(&mut self, key: T, room: &str) -> Result<String, String> {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            if !subscriptions.contains_key(&key) {
                subscriptions.insert(key.clone(), HashSet::new());
            }
            subscriptions
                .get_mut(&key)
                .unwrap() // We know its in there, we just added it above
                .insert(room.to_string());

            Ok(format!(
                "Subscribing to {} on {}",
                key, &self.server_details.domain
            ))
        } else {
            Err(format!("Sorry, I could not add your request {} on {} to the subscriptions, due to an internal error ({}).",
                key, &self.server_details.domain, "subscriptions not lockable"))
        }
    }

    pub fn unsubscribe(&mut self, key: T, room: &str) -> Result<String, String> {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            if !subscriptions.contains_key(&key) {
                return Ok(format!("Was not subscribed to {}", key));
            }
            subscriptions
                .get_mut(&key)
                .unwrap() // We know its in there, we just checked it above
                .remove(room);

            // Check if anybody still uses this key
            if subscriptions.get(&key).unwrap().is_empty() {
                subscriptions.remove(&key);
            }

            Ok(format!(
                "Unsubscribing room from {} on {}",
                key, &self.server_details.domain
            ))
        } else {
            Err(format!("Sorry, I could not remove your request {} on {} from the subscriptions, due to an internal error ({}).",
                key, &self.server_details.domain, "subscriptions not lockable"))
        }
    }

    pub fn scan_line(&self, line: &str) -> ScanLineResult {
        let prefix = self.prefix.as_deref().unwrap_or("");
        if !line.starts_with(prefix) {
            return ScanLineResult::NotForMe;
        }
        // Stripping away the prefix
        let line = line[prefix.len()..].trim();

        if line.starts_with(&format!("list {}", self.subtype)) {
            return ScanLineResult::ListCommand;
        }

        let search_url = format!("{}/{}/", self.server_details.domain, self.subtype);
        // Check if its for me
        if !line.contains(&search_url) {
            return ScanLineResult::NotForMe;
        }

        ScanLineResult::PossiblyForMe
    }

    pub fn handle_message_helper(&mut self, bot: &ActiveBot, message: &str, room: &str) {
        for line in message.lines() {
            match self.scan_line(line) {
                ScanLineResult::PossiblyForMe => { /* Continue below */ }
                ScanLineResult::NotForMe => {
                    continue;
                }
                ScanLineResult::ListCommand => {
                    self.list_keys(bot, room);
                    continue;
                }
            }

            let key = match T::try_from(line.to_string()) {
                Ok(x) => x,
                Err(_) => {
                    println!("Message not parsable");
                    bot.send_message(
                        "Sorry, I could not parse that. Please post a submitrequest URL",
                        room,
                        MessageType::TextMessage,
                    );
                    continue;
                }
            };

            let result = if line.starts_with("unsub") {
                self.unsubscribe(key, room)
            } else {
                self.subscribe(key, room)
            };

            match result {
                // We just send the result-message no matter Ok/Err, but this might
                // change in the future
                Ok(message) | Err(message) => {
                    println!("{}", message);
                    bot.send_message(&message, room, MessageType::TextMessage)
                }
            }
        }
    }

    pub fn subscribe_to_defaults(&mut self, message: &str, room: &str) {
        for line in message.lines() {
            match self.scan_line(line) {
                ScanLineResult::PossiblyForMe => { /* Continue below */ }
                ScanLineResult::NotForMe | ScanLineResult::ListCommand => {
                    continue;
                }
            }

            let key = match T::try_from(line.to_string()) {
                Ok(x) => x,
                Err(_) => {
                    println!("Message {} not parsable", line);
                    continue;
                }
            };

            let result = if !line.starts_with("unsub") {
                self.subscribe(key, room)
            } else {
                continue;
            };

            match result {
                // We just print the result-message no matter Ok/Err, but this might
                // change in the future
                Ok(message) | Err(message) => {
                    println!("{}", message);
                }
            }
        }
    }
}

pub fn subscribe(
    details: &ConnectionDetails,
    channel: Channel,
    subnames: &[&str],
) -> Result<(Channel, Consumer)> {
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

    for key in subnames.iter() {
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

    println!(
        "Subscribing to ({}) on {}",
        subnames.join(", "),
        details.domain
    );

    Ok((channel, consumer))
}

pub fn prepend_prefix(
    prefix: Option<&str>,
    without_prefix: &[(&str, &str)],
) -> Vec<(String, String)> {
    let prefix = prefix.unwrap_or("");

    let mut res = Vec::new();
    for (key, text) in without_prefix.iter() {
        res.push((format!("{}{}", prefix, key), (*text).to_string()));
    }
    res
}
