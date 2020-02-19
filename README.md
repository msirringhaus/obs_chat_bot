# obs_chat_bot

A Matrix chat bot for the open build service (OBS) of openSUSE.

## To test
Create a `botconfig.toml` file like this:
```
user = "botname"
password = "bot_password"
homeserver_url = "https://your.matrix-homeserver.com"
# Optional: Bot only interprets messages starting with this prefix
prefix = "obsbot:"
```

and run `cargo run`.

## To use
Invite your bot into any room (be it 1:1 or group-chat).

Type `[prefix]help` to get more info.

It boils down to: Paste in a URL of a package or a submitrequest to get notifications for changed status.
