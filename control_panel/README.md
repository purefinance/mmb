The crate for remote control of the trading engine via IPC.

Supported requests:
- Health(get): check that the engine is working
- Stop(post)
- Stats(get): getting simple trading statistics
- Config:
   - get(get): get current config
   - set(post): update current config *ENGINE WILL BE REBOOTED*
