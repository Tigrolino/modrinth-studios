# What's in Modrinth Studios

Everything here is on top of the normal Modrinth App. If something isn't listed, it works the same as in the official app.

## Hosting your own servers

The Servers tab is called Hosting here, and besides Modrinth Hosting it can show servers from any [Pterodactyl](https://pterodactyl.io) panel. Each server in the list is labeled so you can tell which is which.

To add a panel, press "Connect panel" on the Hosting page and enter the panel's address and a client API key. You can create the key on your panel under Account → API Credentials, and it starts with `ptlc_`. You don't need a Modrinth account for any of this. If you aren't logged in, the Hosting page just shows your Pterodactyl servers.

Pterodactyl servers open in the same pages Modrinth Hosting uses, so you get:

- the live console, with start, stop and restart
- the file manager, including uploading, editing and zipping or unzipping files
- backups, and schedules for automatic ones
- the content browser, to install, update and remove mods, plugins and datapacks from Modrinth
- installing Modrinth modpacks, and switching the loader or Minecraft version
- startup settings, ports, and a custom address to show instead of the IP
- inviting other users by email and choosing what they're allowed to do
- the panel's activity log, in plain language

Pumpkin servers work too, including plugins.

Studio saves a few things Pterodactyl has no place for, like the list of installed mods, in a hidden `.modrinth-studio` folder on the server. Don't delete it unless you want the app to forget that info.

Some things need an admin key on the panel, like changing a server's egg or its RAM and CPU limits, so the app can't do them. Those options are hidden or show a short note instead.

## Server analytics

Every server, Modrinth Hosting or Pterodactyl, has an Analytics page. It shows:

- CPU, RAM, network and players online, live, plus whether the server responds to pings and how fast
- who joined and when, how long they played, and when the server is busiest, read from the server's logs and shown in your own time zone
- stats from the world itself, like deaths, distance travelled, blocks mined and mobs killed, with records for each player and some fun facts

Live graphs only fill in while the app is open, but the player history and world stats go back as far as the server's files do.

## Play mode

Made for when you mostly play one instance. Turn it on in Settings → Features and pick the instance.

After that, the Play button in the sidebar opens a full-screen page for that instance: a big Play button, your screenshots slowly cycling in the background, your playtime and mod count, and your recent worlds and servers so you can jump right back in. The instance's other tabs are still at the top. Your library moves to its own button under Play.

## Minecraft account in the title bar

Your Minecraft account sits in the top bar instead of the sidebar, so you can switch accounts even when the sidebar is collapsed.

## Replays

A Replays tab lists your ReplayMod and Flashback recordings from every instance in one place. You can see the map, date, players and length of each one, sort and group them, and delete several at once.

## Storage

The Storage page shows how much space each instance actually uses, split into worlds, mods, resource packs and so on.

## Running several instances

You can have more than one instance running at the same time.

## Playtime correction

If an instance crashed or was force closed, its playtime could end up wrong. You can correct it, and the fixed number is used everywhere in the app.

## Appearance

There are a lot more options for how the app looks:

- your own accent color
- a background image
- how see-through and blurred panels are
- a custom window and taskbar icon
- a custom splash screen
- a theme lock, so updates don't reset your look

## Discord status

You can choose the text and images shown on Discord while you play.

## Smaller things

- Settings shows which Modrinth App version this build is based on.
- Banners in the app can be dismissed.
- Renaming an instance doesn't break anything.
- It can be installed alongside the official Modrinth App.

## License

Code from Modrinth keeps its original license, and changes made here use the same license as the code they're in. See the README for details. Modrinth's branding isn't used.
