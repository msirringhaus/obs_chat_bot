use crate::common::{prepend_prefix, ConnectionDetails, MessageParseResult, Subscriber};
use anyhow::Result;
use lapin::{
    message::{Delivery, DeliveryResult},
    options::*,
    Connection, ConsumerDelegate,
};
use matrix_bot_api::handlers::{HandleResult, MessageHandler};
use matrix_bot_api::{ActiveBot, MatrixBot, Message, MessageType};
use serde::Deserialize;
use serde_json;
use std::collections::hash_map::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

const KEY_JOB_DONE: &str = "openqa.job.done";
const SUBNAMES: [&str; 1] = [KEY_JOB_DONE];

pub fn help_str(prefix: Option<&str>) -> Vec<(String, String)> {
    let without_prefix = [
        (
            "OPENQA_TEST_URL",
            "Subscribe to a test. Get notification if test-status changes.",
        ),
        (
            "unsub OPENQA_TEST_URL",
            "Unsubscribe from a test. Get no more notifications.",
        ),
        ("list tests", "List all tests currently subscribed to."),
    ];

    prepend_prefix(prefix, &without_prefix)
}

#[derive(Debug, Clone, std::cmp::PartialEq, std::cmp::Eq, Hash)]
struct QAKey {
    id: String,
}

impl std::fmt::Display for QAKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl TryFrom<String> for QAKey {
    type Error = ();

    fn try_from(line: String) -> Result<Self, Self::Error> {
        let line = line.trim();
        if line.contains('\n') {
            return Err(());
        }

        let parts: Vec<_> = line.split('/').collect();
        if parts.len() < 3 {
            return Err(());
        }

        let mut iter = parts.iter().rev();
        // These unwraps cannot fail, as there have to be at least 2 parts
        let id = iter
            .next()
            .unwrap()
            .trim()
            .trim_end_matches('#')
            .to_string();
        Ok(QAKey { id })
    }
}

#[derive(Deserialize, Debug)]
struct QATestInfo {
    id: i32,
    #[serde(rename = "TEST")]
    testname: String,
    result: String,
    reason: Option<String>,
    // remaining: i32
}

impl MessageHandler for Subscriber<QAKey> {
    /// Will be called for every text message send to a room the bot is in
    fn handle_message(&mut self, bot: &ActiveBot, message: &Message) -> HandleResult {
        let res = self.handle_message_helper(bot, &message.body, &message.room);

        if res == MessageParseResult::SomethingForMe {
            match self.register() {
                Err(x) => {
                    println!("Error while registering: {:?}", x);
                }
                Ok(consumer) => consumer.set_delegate(Box::new(self.clone())),
            }
        }
        HandleResult::ContinueHandling
    }
}

impl Subscriber<QAKey> {
    fn generate_messages(&self, jsondata: QATestInfo) -> (String, String) {
        let reason = match jsondata.reason {
            Some(x) => format!(" (reason: {})", x),
            None => String::new(),
        };

        let html_result = match &jsondata.result {
            x if x == "passed" => x.clone(),
            x => format!("<u>{}</u>", x),
        };

        let plain = format!(
            "Test {}: {} ({}){}",
            &jsondata.result, jsondata.testname, jsondata.id, reason
        );

        let html = format!(
            "<strong>Test {}:</strong> Test {} (<a href={}>{}</a>){}",
            html_result,
            jsondata.testname,
            format!("{}/{}", self.get_base_url(), jsondata.id,),
            jsondata.id,
            reason
        );

        (plain, html)
    }

    fn delivery_wrapper(&self, delivery: Delivery) -> Result<()> {
        let data = std::str::from_utf8(&delivery.data)?;
        let jsondata: QATestInfo = serde_json::from_str(data)?;

        let key = QAKey {
            id: format!("{}", jsondata.id),
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

        println!("Test {}: {}", jsondata.result, jsondata.id);

        if let Ok(bot) = self.bot.lock() {
            let (plain, html) = self.generate_messages(jsondata);
            for room in &rooms {
                bot.send_html_message(&plain, &html, room, MessageType::TextMessage);
            }
        }

        Ok(())
    }
}

impl ConsumerDelegate for Subscriber<QAKey> {
    fn on_new_delivery(&self, delivery: DeliveryResult) {
        if let Ok(Some(delivery)) = delivery {
            if let Some(channel) = &self.channel {
                let _ = channel
                    .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
                    .wait();
            }
            match self.delivery_wrapper(delivery) {
                Ok(_) => {}
                Err(x) => println!("Error while getting Event: {:?}. Skipping to continue", x),
            }
        } else {
            println!(
                "Delivery not ok on {}: {:?}",
                self.server_details.domain, delivery
            );
        }
    }
}

pub fn init(
    bot: &mut MatrixBot,
    details: &ConnectionDetails,
    conn: Connection,
    prefix: Option<String>,
    default_subs: &Option<Vec<(String, String)>>,
) -> Result<()> {
    let activebot = bot.get_activebot_clone();
    let mut server_details = *details;
    server_details.buildprefix = "openqa";
    let mut sub: Subscriber<QAKey> = Subscriber {
        subtype: "tests".to_string(),
        server_details,
        connection: conn,
        channel: None,
        subnames: SUBNAMES.to_vec(),
        bot: Arc::new(Mutex::new(activebot)),
        subscriptions: Arc::new(Mutex::new(HashMap::new())),
        prefix,
    };

    match default_subs {
        None => {}
        Some(subs) => match sub.register() {
            Err(_) => {}
            Ok(consumer) => {
                consumer.set_delegate(Box::new(sub.clone()));
                for (room, url) in subs {
                    sub.subscribe_to_defaults(&url, &room);
                }
            }
        },
    }
    bot.add_handler(sub);

    Ok(())
}
