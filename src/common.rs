use anyhow::Result;
use lapin::{options::*, types::FieldTable, Channel, Consumer, ExchangeKind};
use matrix_bot_api::{ActiveBot, Message, MessageType};
use std::collections::{HashMap, HashSet};
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
    T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Debug,
{
    pub server_details: ConnectionDetails,
    pub channel: Channel,
    pub bot: Arc<Mutex<ActiveBot>>,
    pub subscriptions: Arc<Mutex<HashMap<T, HashSet<String>>>>,
}

impl<T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Debug> Subscriber<T> {
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
                "Subscribing room {} to {:?} on {}",
                room, key, &self.server_details.domain
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
                "Unsubscribing room {} to {:?} on {}",
                room, key, &self.server_details.domain
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
        search_url: &str,
        min_splits: usize,
        key_parser_func: Box<dyn Fn(&Vec<&str>) -> T>,
    ) {
        for line in message.body.lines() {
            // Check if its for me
            if !line.contains(search_url) {
                continue;
            }

            let line = line.trim();

            let parts: Vec<_> = line.split("/").collect();
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

    println!("Subscribing to {}", details.domain);

    Ok((channel, consumer))
}
