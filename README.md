# obs_chat_bot

A Matrix chat bot for the open build service (OBS) of openSUSE.

## To test
Create a `obs_chat_bot/config.toml` file in one of your XDG_CONFIG-directories like this:
```
user = "botname"
password = "bot_password"
homeserver_url = "https://your.matrix-homeserver.com"
# Only opensuse.org and suse.de supported at the moment
backends = ["opensuse.org", "suse.de"]  

# Optional: Bot only interprets messages starting with this prefix
prefix = "obsbot:"
```

and run `cargo run`.

or

give it directly with `cargo run -- YOURCONFIG.toml`

## To use
Invite your bot into any room (be it 1:1 or group-chat).

Type `[prefix]help` to get more info.

It boils down to: Paste in a URL of a package or a submitrequest to get notifications for changed status.
