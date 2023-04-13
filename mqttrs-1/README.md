# mqttrs-1

This is an initial attempt to implement an MQTT broker and client using rust and mqttrs.

## Research

### Decision to Choose mqttrs

This reddit thread about [which mqtt rust library to use](https://www.reddit.com/r/rust/comments/g2c75e/which_mqtt_rust_library_do_you_recommend/) seemed useful. The most popular mqtt rust library seems to be paho-mqtt, but the question answerer preferred mqttrs for reasons of code cleanliness, documentation, and best-tested-ness.

Useful qualities of the mqttrs library also include that it has fewer dependencies (mqttrs, bytes, and all the dependencies those pull in). It is not a wrapper around a pre-compiled library (for example, paho-mqtt is an "FFI wrapper"). This is important because we are prioritizing keeping dependencies (and overall code-size) small. Pulling in dependencies that are closed-source is also a no-go. Endorsement for quality of documentation and ease of use (API simplicity) are good.

### Tokio

This is a rust library for asynchronous code. The mqttrs crate is designed to work with tokio, and it will probably be necessary to pull in this library eventually if we want to allow for asynchronous communication between MQTT clients.

### MQTT CLI by HiveMQ

I was turned on to this tool by Hassam Uddin's MQTT tutorial, Part 2. This is a CLI tool to simulate being an MQTT cilent. Very useful for testing.

#### Using MQTT CLI on OpenBSD

To use MQTT CLI with the mqttrs-1 project, there are a couple of options:

##### Option A: Installing MQTT CLI on OpenBSD (NOT WORKING 04/12/23)

They don't have precompiled packages for Unix. So if we want this, it will need to be built from source.

1. Install java. Openbsd.app has OpenJDK. `does pkg_add jdk`. (I picked version 17, when asked)
1. Add `java` to your `PATH` and set `JAVA_HOME`.

   ```
   cat "export JAVA_HOME=/usr/local/<jdk-dir>" >> $HOME/.profile
   . $HOME/.profile
   echo $JAVA_HOME
   ```

   ```
   # inside $HOME/.profile
   PATH=<stuff>:/usr/local/<jdk-dir>/bin/java
   ```
1. If you're using a desktop environment (like XFCE), you may have to change the terminal emulator settings. For XFCE, for instance, go to Edit > Preferences > Run as Login shell. This will make each new window automatically source the ~/.profile script.

1. Clone the HiveMQ MQTT CLI repository and cd into it.

1. Start installation: `./gradlew installNativeImageTooling`

   > ERR: this does not seem to work on openbsd. Fails with an error message that says "SystemInfo is not supported for this operating system"

##### Option B: Install MQTT CLI on another computer and connect to external host

1. Install on another (non-openbsd) computer.

1. Open mqtt-cli and connect: `con -V3 -h=<ip-address-of-openbsd> -p=1883`

#### Communicating to OpenBSD on Oracle VirtualBox VM from Windows 10 Host

My current setup is using openbsd on a virtual machine from Windows. This requires some extra steps to get both operating systems talkling to each other.

1. Power down the VM and create 2 network adapters for the OpenBSD VM. Click on "Machine > Settings > Network". Adapter 1 should already be NAT. Enable Adapter 2 and make this a Bridged Adapter and attach it to the NIC of the host computer.

1. Forward ports from the Guest OS to the Host OS. [This guide](https://www.xmodulo.com/access-nat-guest-from-host-virtualbox.html) was really helpful. Under "Machine > Settings > Network > Adapter 2", click on "Port Forwarding" and add an entry:

   > Name: MQTT
   > Protocol: TCP
   > Host IP: 127.0.0.1
   > Host Port: \<any unused port, 8083, for instance\>
   > Guest IP: \<Guest OS ip address, probably, 10.0.2.15\>
   > Guest Port: 1883

For the guest port, it is 1883 because that is the default MQTT protocol port. If you want to use a different port, this number must be changed.

1. Restart the virtual machine.

1. You can check if its working by starting the mqttrs-1 project and then running `netstat -na -f inet` in another terminal window. There should be a TCP socket listening for 1883.

1. On the Host OS, run the MQTT CLI and connect: `con -V3 -h=127.0.0.1 -p=8083`. Where the port argument is the "Host Port" used in the port forwarding configuration.

## Resources and Links

1. [Writing an asynchronous MQTT Broker in Rust - Part 1](https://hassamuddin.com/blog/rust-mqtt/overview/)
2. [MQTT Version 3.1.1 Spec](http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html)
3. [Tokio (Getting Started)](https://tokio.rs/tokio/tutorial/hello-tokio)
4. [HiveMQ's MQTT CLI](https://hivemq.github.io/mqtt-cli/docs/installation/)
5. [Access NAT Guest from Host VirtualBox](https://www.xmodulo.com/access-nat-guest-from-host-virtualbox.html)
