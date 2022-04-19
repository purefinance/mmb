The crate for remote control of the trading engine via IPC.

Supported http requests:
- Health(get): check that the engine is working
- Stop(post)
- Stats(get): getting simple trading statistics
- Config:
   - get(get): get current config
   - set(post): update current config *ENGINE WILL BE REBOOTED*

After editing endpoints you should update swagger config.
There is no stable config swagger generator for rust code. Therefore use https://editor.swagger.io/#/ for editing manually `http_api.json` in path [control_panel/webui/http_api.json](../control_panel/webui/http_api.json)