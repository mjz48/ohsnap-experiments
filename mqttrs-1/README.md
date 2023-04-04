# mqttrs-1

This is an initial attempt to implement an MQTT broker and client using rust and mqttrs.

## Research

### Decision to Choose mqttrs

This reddit thread about [which mqtt rust library to use](https://www.reddit.com/r/rust/comments/g2c75e/which_mqtt_rust_library_do_you_recommend/) seemed useful. The most popular mqtt rust library seems to be paho-mqtt, but the question answerer preferred mqttrs for reasons of code cleanliness, documentation, and best-tested-ness.

Useful qualities of the mqttrs library also include that it has fewer dependencies (mqttrs, bytes, and all the dependencies those pull in). It is not a wrapper around a pre-compiled library (for example, paho-mqtt is an "FFI wrapper"). This is important because we are prioritizing keeping dependencies (and overall code-size) small. Pulling in dependencies that are closed-source is also a no-go. Endorsement for quality of documentation and ease of use (API simplicity) are good.

## Resources and Links

1. [Writing an asynchronous MQTT Broker in Rust - Part 1](https://hassamuddin.com/blog/rust-mqtt/overview/)
2. [MQTT Version 3.1.1 Spec](http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html)
