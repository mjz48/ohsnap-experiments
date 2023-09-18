# mqttrs-1

This is an initial attempt to implement an MQTT broker and client using rust and mqttrs.

## Usage

The broker accepts several command line flags:

```
#> cargo run -- -h

Usage: mqttrs-1 [OPTIONS]

Options:
  -p, --port <TCP/IP SOCKET>       Port num for broker to listen on
  -i, --ip <IP ADDRESS>            IP Address for broker to listen on
  -m, --max_retries <MAX RETRIES>  Maximum number of packet retranmission attempts before aborting. Will default to infinite retries.
  -r, --retry <DURATION>           Time to wait before re-sending QoS>0 packets (in seconds).
  -t, --timeout <DURATION>         Default timeout interval. E.g. for connections, etc. (in seconds). Separate form QoS retry interval.
  -v, --verbosity <VERBOSITY>      Specify log level verbosity (values=off|error|warn|info|debug|trace)
  -h, --help                       Print help
  -V, --version                    Print version
```

One may try out the functionality by running an instance of the broker, and then starting one or more of the mqtt-cli programs in separate windows. Once connection has been established, one may use the cli to send and receive messages with the publish and subscribe commands, respectively.

### Publish/Subscribe Example on Local computer

In one terminal window:

```
# in mqttrs-1

#> cargo run -- -v trace
```

In two separate terminal windows:

```
# in mqtt-cli

#> cargo run
#> connect -u client1
#> subscribe alerts notifications msg/incoming/new
```

```
# in mqtt-cli

#> cargo run
#> connect -u client2

#> publish alerts "HELLO WORLD"
```

One would expect to see something like this in client1's messages:

```
Connected to the server!
client1@127.0.0.1:1883> subscribe alerts notifications msg/incoming/new
Waiting on messages for subscribed topics...
To exit, press Ctrl-c.
['alerts']: HELLO WORLD
```

## TODO

* Implement Pid handling and creation
* Implement SubscribeTopics and QoS selection
* Implement wildcards in topic paths and ### (publish to all)
* Implement last will handling
* Implement session takeover and QoS retry in that case.
* Implement keep alive detection

## MQTT Spec Notes

### QoS = 1 (AtLeastOnce) Flow

#### Sender Flow

1. Publish sent. Save packet somewhere and retry x times on timeout.
1. Puback received. Close transaction, abort in progress retries.

#### States
'->' means state transition happens with no need for external input

  * PublishSent (retry on timeout)
  * PubackReceived -> Completed

#### Receiver Flow (no data structure needed)

1. Publish received. Send puback, close transaction.

### QoS = 2 (ExactlyOnce) Flow

#### Sender Flow

```
[start] -> Publish -> Pubrec -> Pubrel -> Pubcomp -> [complete]
```

1. Send publish. Save packet somewhere and retry x times on timeout.
1. Pubrec received.
1. Send pubrel. Save pubrel somewhere and retry x times on timeout.
1. Pubcomp received. Close transaction, abort in progress retries.

#### States
'->' means state transition happens with no need for external input

  * PublishSent (retry on timeout)
  * PubrecReceived -> PubrelSent (retry on timeout)
  * PubcompReceived -> Completed

#### Receiver Flow

```
[start] -> Pubrec -> Pubrel -> Pubcomp -> [complete]
```

1. Publish received.
1. Send pubrec. Save packet somewhere and retry x times on timeout.
1. Pubrel received.
1. Send pubcomp. Close transaction, abort in progress retries.

#### States
'->' means state transition happens with no need for external input

  * PublishReceived -> PubrecSent (retry on timeout)
  * PubrelReceived -> PubcompSent -> Completed

## Research

### Decision to Choose mqttrs

This reddit thread about [which mqtt rust library to use](https://www.reddit.com/r/rust/comments/g2c75e/which_mqtt_rust_library_do_you_recommend/) seemed useful. The most popular mqtt rust library seems to be paho-mqtt, but the question answerer preferred mqttrs for reasons of code cleanliness, documentation, and best-tested-ness.

Useful qualities of the mqttrs library also include that it has fewer dependencies (mqttrs, bytes, and all the dependencies those pull in). It is not a wrapper around a pre-compiled library (for example, paho-mqtt is an "FFI wrapper"). This is important because we are prioritizing keeping dependencies (and overall code-size) small. Pulling in dependencies that are closed-source is also a no-go. Endorsement for quality of documentation and ease of use (API simplicity) are good.

### Tokio

This is a rust library for asynchronous code. The mqttrs crate is designed to work with tokio, and it will probably be necessary to pull in this library eventually if we want to allow for asynchronous communication between MQTT clients.

For completely new people, there is a lot of required reading before being able to fully understand and utilize the tokio library (or to hand-roll your own async code). Here's a recommended list of topics:

1. The [rust book](https://doc.rust-lang.org/book/). Probably at least up to chapter 16.
1. The [rust async book](https://rust-lang.github.io/async-book/). 
1. The [tokio tutorial](https://tokio.rs/tokio/tutorial).

### Asynchronous Rust Programming (and can we reduce dependencies by rolling our own stuff?)

Rust async programming is still evolving and subject to change. By their own admission (in the foreword of the rust async book), async programming is harder and more error prone than synchronous rust. The ecosystem is also strange, being reliant on a mixture of "official" crates such as `futures` and `async-std` and really popular third party crates like `tokio`, `mio`, and `smol`.

So it seems like quite a difficult task to want to make a asynchronous program in Rust without using third-party packages whatsoever.

#### Motivation for using Asynchronous programming

The MQTT broker by nature requires many asynchronous elements and is expected to handle a lot of clients. In a small system, this may be only one or two, but typically can be in the hundreds, or even thousands. For OHSNAP purposes, we may be working on someone's home network (~O(10) devices?) but don't want to exclude corporate or organizational use-cases (~O(100+)?). So scaling up the number of clients (and the threads they would require) is a legitimate concern here.

Asynchronous code is more performant than using OS threads to do this. And compared to coroutines or actors, the async paradigm is chosen as the recommended method for doing this type of thing by the official Rust people.

#### How do the third party async runtimes work?

https://kerkour.com/rust-async-await-what-is-a-runtime

This seems like a useful resource. Once one grasps how to use the async libraries, the next logical step might be to try and write your own.

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
1. [MQTT Version 3.1.1 Spec](http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html)
1. [Tokio (Getting Started)](https://tokio.rs/tokio/tutorial/hello-tokio)
1. [HiveMQ's MQTT CLI](https://hivemq.github.io/mqtt-cli/docs/installation/)
1. [Access NAT Guest from Host VirtualBox](https://www.xmodulo.com/access-nat-guest-from-host-virtualbox.html)
