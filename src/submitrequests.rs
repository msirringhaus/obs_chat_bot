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
use std::sync::{Arc, Mutex};

const KEY_REQUEST_CHANGE: &str = "obs.request.change";
const KEY_REQUEST_STATECHANGE: &str = "obs.request.state_change";
const KEY_REQUEST_DELETE: &str = "obs.request.delete";
const KEY_REQUEST_COMMENT: &str = "obs.request.comment";

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

impl MessageHandler for Subscriber<String> {
    /// Will be called for every text message send to a room the bot is in
    fn handle_message(&mut self, bot: &ActiveBot, message: &Message) -> HandleResult {
        let keyparser = |parts: &Vec<&str>| {
            let mut iter = parts.iter().rev();
            // These unwraps cannot fail, as there have to be at least 2 parts
            iter.next().unwrap().trim().to_string()
        };
        self.handle_message_helper(bot, message, 3, Box::new(keyparser));

        HandleResult::ContinueHandling
    }
}

impl Subscriber<String> {
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

        let key = format!("{}", jsondata.number);

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

        self.channel
            .basic_ack(delivery.delivery_tag, BasicAckOptions::default())
            .wait()?;

        Ok(())
    }
}

impl ConsumerDelegate for Subscriber<String> {
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

pub fn subscribe(
    bot: &mut MatrixBot,
    details: &ConnectionDetails,
    channel: Channel,
    prefix: Option<String>,
) -> Result<()> {
    let subnames = [
        KEY_REQUEST_CHANGE,
        KEY_REQUEST_STATECHANGE,
        KEY_REQUEST_DELETE,
        KEY_REQUEST_COMMENT,
    ];
    let (channel, consumer) = crate::common::subscribe(details, channel, &subnames)?;
    let sub: Subscriber<String> = Subscriber {
        subtype: "request".to_string(),
        server_details: *details,
        channel,
        bot: Arc::new(Mutex::new(bot.get_activebot_clone())),
        subscriptions: Arc::new(Mutex::new(HashMap::new())),
        prefix,
    };
    bot.add_handler(sub.clone());
    consumer.set_delegate(Box::new(sub));

    Ok(())
}
