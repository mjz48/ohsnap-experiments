# mqtt-cli

Roll up an MQTT CLI in rust. Can't find one that doesn't require dependencies like gradle (i.e. written in Java), or electron. This needs to be able to be run on OpenBSD.

## Research

### Prior Art

This project will reference HiveMQ Mqtt-cli, which requires jdk and gradle to build, and does not have precompiled binaries for OpenBSD. Trying to build Mqtt-cli from source using gradle on OpenBSD results in errors because OpenBSD gradle does not support "SystemInfo".

Several other projects seemed promising but had no documentation and appeared to be early prototypes. (i.e. mqttc rust crate)

Other projects were not cli tools, which is what we need right now for testing.

## Resources and Links

1. [HiveMQ Mqtt-cli](https://github.com/hivemq/hivemq-mqtt-client)
1. [Rust-mq](https://github.com/inre/rust-mq)
1. [mqttc](https://docs.rs/mqttc/0.1.3/mqttc/)
