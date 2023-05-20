# mqtt-cli

Roll up an MQTT CLI in rust. Can't find one that doesn't require dependencies like gradle (i.e. written in Java), or electron. This needs to be able to be run on OpenBSD.

## TODO

* Implement flags on connect command (hostname, port, keep\_alive, qos, last\_will, etc)
* Change shell prompt to use hostname and port when connected?
* Implement more commands: Disconnect, Create topic, subscribe, publish

## Open issues

* Exiting the cli tool will many times hang and then require the user to hit enter or Ctrl+C again, and then dump them back into the terminal shell with a message about not being able to send over a closed connection. The reason this happens is that the shell.rs run() method has two threads and the execution thread exits upon exit() but the input thread may be blocked waiting on io::stdin and that's the reason why it hangs. There's no easy/quick/simple way to fix this.

## Research

* Rust is turning out to be quite a bear when it comes to creating a software architecture to solve a domain problem. The static typing and strict management of variable ownership and lifetimes means that a slight change or unexpected read/write access of a variable can completely change the architecture. Slight modifications to the functionality can completely shut off entire streams of possibilities. Some stack overflow advice for this is to start from the bottom up. Get something small working. And then another thing. And then integrate the pieces. And refactor repeatedly until the pieces fit together with something workable. Do that until the whole program is complete.

* Not sure what the velocity of coding in rust is, since I'm still pretty new. But so far it has been very slow. Trying to create a shell and state object and passing them around has had me run into lifetime issues and synchronization difficulties.

### Prior Art

This project will reference HiveMQ Mqtt-cli, which requires jdk and gradle to build, and does not have precompiled binaries for OpenBSD. Trying to build Mqtt-cli from source using gradle on OpenBSD results in errors because OpenBSD gradle does not support "SystemInfo".

Several other projects seemed promising but had no documentation and appeared to be early prototypes. (i.e. mqttc rust crate)

Other projects were not cli tools, which is what we need right now for testing.

## Resources and Links

1. [HiveMQ Mqtt-cli](https://github.com/hivemq/hivemq-mqtt-client)
1. [Rust-mq](https://github.com/inre/rust-mq)
1. [mqttc](https://docs.rs/mqttc/0.1.3/mqttc/)
