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

# Optional: default subscriptions, to subscribe to at startup. List of (room, URL) to go through
#           room: That is the matrix interal room-key. You can get this usually via the room-settings under "Advanced"
# Note: Error-handling is minimal here. Errors in URLs or rooms won't cause aborts, but simply no or wrong subscriptions.
default_subs = [["!sIdZOJxxgKCJANAvTJ:your-matrix-server.org", "https://build.opensuse.org/request/show/777777"],
                ["!sIdZOJxxgKCJANAvTJ:your-matrix-server.org", "https://build.suse.de/package/show/home:YOU/hello_world"]]
```

and run `cargo run`.

or

give it directly with `cargo run -- YOURCONFIG.toml`

## To use
Invite your bot into any room (be it 1:1 or group-chat).

Type `[prefix]help` to get more info.

It boils down to: Paste in a URL of a package or a submitrequest to get notifications for changed status.
