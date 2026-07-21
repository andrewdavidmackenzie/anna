## Using Anna in Local Mode

### Starting Anna processes using the rust CLi
You can start anna background processes by using the `anna` CLI `start` command

If you have the `anna` crate installed, just use
```bash
> anna start
```

If you have not installed it, then you can build and run the development version using
```bash
> cargo run -- start
```

By default, the `server/conf/anna-config.yml` config file is used, which only specifies one routing thread and one storage
thread.

You are welcome to modify this file if you would like, but we generally do not recommend running more than one
thread per process in local mode.

### Stopping Anna

You can stop the running background processes by using the `anna` CLI `stop` command

If you have the `anna` crate installed, just use
```bash
> anna stop
```

If you have not installed it, then you can build and run the development version using
```bash
> cargo run -- stop
```

## Running Anna in cluster mode

For instructions on how to run Anna in cluster mode, please see the `hydro-project/cluster`
[repository](https://github.com/hydro-project/cluster).