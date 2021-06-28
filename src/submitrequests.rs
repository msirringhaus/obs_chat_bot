use crate::common::{prepend_prefix, ConnectionDetails, MessageParseResult, Subscriber};
use anyhow::{anyhow, Result};
use lapin::{
    message::{Delivery, DeliveryResult},
    options::*,
    Connection, ConsumerDelegate,
};
use matrix_bot_api::handlers::{HandleResult, MessageHandler};
use matrix_bot_api::{ActiveBot, MatrixBot, Message, MessageType};
use serde::Deserialize;

use std::collections::hash_map::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, Mutex};

const KEY_REQUEST_CHANGE: &str = "obs.request.change";
const KEY_REQUEST_STATECHANGE: &str = "obs.request.state_change";
const KEY_REQUEST_DELETE: &str = "obs.request.delete";
const KEY_REQUEST_COMMENT: &str = "obs.request.comment";
const SUBNAMES: [&str; 4] = [
    KEY_REQUEST_CHANGE,
    KEY_REQUEST_STATECHANGE,
    KEY_REQUEST_DELETE,
    KEY_REQUEST_COMMENT,
];

#[derive(Debug, Clone, std::cmp::PartialEq, std::cmp::Eq, Hash)]
struct RequestKey {
    id: String,
}

impl std::fmt::Display for RequestKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl TryFrom<String> for RequestKey {
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
        let id = iter.next().unwrap().trim().to_string();
        Ok(RequestKey { id })
    }
}

pub fn help_str(prefix: Option<&str>) -> Vec<(String, String)> {
    let without_prefix = [
        (
            "OBS_REQUEST_URL",
            "Subscribe to a SR/MR. Get notification if state changes.",
        ),
        (
            "unsub OBS_REQUEST_URL",
            "Unsubscribe from a SR/MR. Get no more notifications.",
        ),
        (
            "list requests",
            "List all requests currently subscribed to.",
        ),
    ];

    prepend_prefix(prefix, &without_prefix)
}

#[derive(Deserialize, Debug)]
struct SubmitRequestInfo {
    state: String,
    number: i32,
    author: Option<String>,
    comment: Option<String>,
    comment_body: Option<String>,
    commenter: Option<String>,
    description: Option<String>,
    actions: Option<serde_json::Value>,
    when: Option<String>,
    who: Option<String>,
    oldstate: Option<String>,
}

impl MessageHandler for Subscriber<RequestKey> {
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

impl Subscriber<RequestKey> {
    fn generate_messages(&self, jsondata: SubmitRequestInfo, changetype: &str) -> (String, String) {
        let mut commentfield = String::new();
        if changetype == "commented" {
            if jsondata.commenter.is_some() {
                commentfield += jsondata.commenter.as_ref().unwrap();
                commentfield += ": ";
            }

            if jsondata.comment_body.is_some() {
                commentfield += jsondata.comment_body.as_ref().unwrap();
            }
        } else if jsondata.comment.is_some() {
            commentfield += jsondata.comment.as_ref().unwrap();
        }

        let plain = format!(
            "Request {} was {}. Status: {} ({})",
            jsondata.number, changetype, jsondata.state, commentfield,
        );
        let html = format!(
            "<a href={}>Request {}</a> was {}. Status <strong>{}</strong> {}",
            format!("{}/{}", self.get_base_url(), jsondata.number,),
            jsondata.number,
            changetype,
            jsondata.state,
            if commentfield.is_empty() {
                String::new()
            } else {
                format!("<br>{}", commentfield)
            }
        );

        (plain, html)
    }

    fn delivery_wrapper(&self, delivery: Delivery) -> Result<()> {
        let data = std::str::from_utf8(&delivery.data)?;
        let jsondata: SubmitRequestInfo = serde_json::from_str(data)?;
        let changetype;
        if delivery.routing_key.as_str().contains(KEY_REQUEST_CHANGE) {
            changetype = "changed by admin";
        } else if delivery
            .routing_key
            .as_str()
            .contains(KEY_REQUEST_STATECHANGE)
        {
            changetype = "changed";
        } else if delivery.routing_key.as_str().contains(KEY_REQUEST_DELETE) {
            changetype = "deleted";
        } else if delivery.routing_key.as_str().contains(KEY_REQUEST_COMMENT) {
            changetype = "commented";
        } else {
            return Err(anyhow!(
                "Changetype of SR event unknown: {}",
                delivery.routing_key.as_str()
            ));
        }

        let key = RequestKey {
            id: format!("{}", jsondata.number),
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

        println!("Request got {}: {}", changetype, jsondata.number);

        if let Ok(bot) = self.bot.lock() {
            let (plain, html) = self.generate_messages(jsondata, changetype);
            for room in &rooms {
                bot.send_html_message(&plain, &html, room, MessageType::TextMessage);
            }
        }

        Ok(())
    }
}

impl ConsumerDelegate for Subscriber<RequestKey> {
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
    let mut sub: Subscriber<RequestKey> = Subscriber {
        subtype: "request".to_string(),
        server_details: *details,
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
