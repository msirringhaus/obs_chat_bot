# obs_chat_bot

A Matrix chat bot for the open build service (OBS) of openSUSE.

## Install
obs_chat_bot needs a config-file with login credentials and other settings.

You can copy the `example_config.toml` and fill in the blanks.

This file can be moved to one of your XDG_CONFIG-directories, or given directly to the bot as a commandline-argument.

### From source
Clone this repo, then run `cargo run`.

or

give it directly a config-file with `cargo run -- YOURCONFIG.toml`

### From OBS

Go to https://build.opensuse.org/package/show/home:MSirringhaus/obs_chat_bot and either add it as a repository or download the RPM directly.

After intall, start the bot simply with `obs_chat_bot`

or

give it directly a config-file with `obs_chat_bot YOURCONFIG.toml`

## Usage
Invite your bot into any room (be it 1:1 or group-chat).

Type `[prefix]help` to get more info.

It boils down to: Paste in a URL of a package or a submitrequest to get notifications for changed status.
