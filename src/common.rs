use anyhow::Result;
use lapin::{options::*, types::FieldTable, Channel, Consumer, ExchangeKind};
use matrix_bot_api::{ActiveBot, Message, MessageType};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

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
    T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Display,
{
    pub server_details: ConnectionDetails,
    pub channel: Channel,
    pub bot: Arc<Mutex<ActiveBot>>,
    pub subscriptions: Arc<Mutex<HashMap<T, HashSet<String>>>>,
    pub prefix: Option<String>,
    pub subtype: String,
}

impl<T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Display> Subscriber<T> {
    pub fn get_base_url(&self) -> String {
        format!(
            "https://{}.{}/{}/show",
            self.server_details.buildprefix, self.server_details.domain, self.subtype
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

    pub fn subscribe(&mut self, key: T, bot: &ActiveBot, room: &str) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            if !subscriptions.contains_key(&key) {
                subscriptions.insert(key.clone(), HashSet::new());
            }
            subscriptions
                .get_mut(&key)
                .unwrap() // We know its in there, we just added it above
                .insert(room.to_string());
            println!(
                "Subscribing room {} to {} on {}",
                room, key, &self.server_details.domain
            );

            bot.send_message(
                &format!("Subscribing to {} on {}", key, &self.server_details.domain),
                room,
                MessageType::TextMessage,
            );
        } else {
            println!("subscriptions not lockable");
            bot.send_message(
                "Sorry, I could not add your request to the subscriptions, due to an internal error.",
                room,
                MessageType::TextMessage,
            );
        }
    }

    pub fn unsubscribe(&mut self, key: T, bot: &ActiveBot, room: &str) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            if !subscriptions.contains_key(&key) {
                return;
            }
            subscriptions
                .get_mut(&key)
                .unwrap() // We know its in there, we just checked it above
                .remove(room);

            // Check if anybody still uses this key
            if subscriptions.get(&key).unwrap().is_empty() {
                subscriptions.remove(&key);
            }

            println!(
                "Unsubscribing room {} to {} on {}",
                room, key, &self.server_details.domain
            );

            bot.send_message(
                &format!(
                    "Unsubscribing to {} on {}",
                    key, &self.server_details.domain
                ),
                room,
                MessageType::TextMessage,
            );
        } else {
            println!("subscriptions not lockable");
            bot.send_message(
                "Sorry, I could not remove your subscription, due to an internal error.",
                room,
                MessageType::TextMessage,
            );
        }
    }

    pub fn handle_message_helper(
        &mut self,
        bot: &ActiveBot,
        message: &Message,
        min_splits: usize,
        key_parser_func: Box<dyn Fn(&Vec<&str>) -> T>,
    ) {
        for line in message.body.lines() {
            let prefix = self.prefix.as_deref().unwrap_or("");
            if !line.starts_with(prefix) {
                continue;
            }
            // Stripping away the prefix
            let line = line[prefix.len()..].trim();

            if line == format!("list {}s", self.subtype) {
                self.list_keys(bot, &message.room);
                continue;
            }

            let search_url = format!("{}/{}/", self.server_details.domain, self.subtype);
            // Check if its for me
            if !line.contains(&search_url) {
                continue;
            }

            let parts: Vec<_> = line.split('/').collect();
            if parts.len() < min_splits {
                println!("Message not parsable");
                bot.send_message(
                    "Sorry, I could not parse that. Please post a submitrequest URL",
                    &message.room,
                    MessageType::TextMessage,
                );
                continue;
            }

            let key = key_parser_func(&parts);

            if line.starts_with("unsub") {
                self.unsubscribe(key, bot, &message.room);
            } else {
                self.subscribe(key, bot, &message.room);
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
