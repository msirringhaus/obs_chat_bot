use anyhow::Result;
use lapin::{options::*, types::FieldTable, Channel, ExchangeKind};
use matrix_bot_api::{ActiveBot, MatrixBot, MessageType};
use std::collections::hash_map::HashMap;
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
    pub subscriptions: Arc<Mutex<HashMap<T, Vec<String>>>>,
}

impl<T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Debug> Subscriber<T> {
    pub fn add_to_subscriptions(&mut self, key: T, bot: &ActiveBot, room: &str) {
        if let Ok(mut subscriptions) = self.subscriptions.lock() {
            if !subscriptions.contains_key(&key) {
                subscriptions.insert(key.clone(), Vec::new());
            }
            subscriptions
                .get_mut(&key)
                .unwrap() // We know its in there, we just added it above
                .push(room.to_string());
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
}

pub fn subscribe<T>(
    bot: &mut MatrixBot,
    details: &ConnectionDetails,
    channel: Channel,
    subnames: &[&str],
) -> Result<()>
where
    T: Send + Clone + std::hash::Hash + std::cmp::Eq + core::fmt::Debug,
{
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

    let sub: Subscriber<(String, String)> = Subscriber {
        server_details: details.clone(),
        channel: channel,
        bot: Arc::new(Mutex::new(bot.get_activebot_clone())),
        subscriptions: Arc::new(Mutex::new(HashMap::new())),
    };
    bot.add_handler(sub.clone());
    consumer.set_delegate(Box::new(sub));

    Ok(())
}
