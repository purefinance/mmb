fn run_server() {
    let mut io = MetaIoHandler::<()>::default();
    io.add_sync_method("say_hello", |_params| {
        Ok(Value::String("hello grays".to_string()))
    });

    let builder = ServerBuilder::new(io);
    let server = builder
        .start("/tmp/json-ipc-test.ipc")
        .expect("Couldn't open socket");
    server.wait();
}
