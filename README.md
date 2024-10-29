# airgradient_monitor
Daemon to pull JSON data directly from airgradient over HTTP and insert it in InfluxDB.

See the example file `src/airgradient_monitor.toml` for how to configure. By default this file will be read from `/etc/airgradient_monitor.toml`, or you can pass the path on the commandline as the first argument.

I wrote this for my own use because it seemed way simpler and more flexible than modifying the airgradient firmware. I'm using it with a Open Air, model O-1PST. What matters is the JSON format though.

Note that I literally wrote this this morning 2024-10-28 and am published it today, so while it is running fine it is not well tested, and the error handling is pretty weak. I'm just putting it out there in case it helps someone else.
