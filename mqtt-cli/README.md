# mqtt-cli

Roll up an MQTT CLI in rust. Can't find one that doesn't require dependencies like gradle (i.e. written in Java), or electron. This needs to be able to be run on OpenBSD.

## TODO

* Implement topic path validation (needed for publish, subscribe, and unsubscribe)
* Implement Pid handling
* Implement last will
* Implement dup, retain, session

### P1 features:

* Change tcp.rs to use one or two 'static buffers instead of creating small buffers for each packet. That more closely resembles the intended usage of mqttrs (and is probably more performant than allocating tiny packet sized buffers for every transmission).
* Implement more POSIX like command line flags
    * short flags can be combined (e.g. -rdq)
    * short flags with optional/required parameters can have no space (e.g. -tflagval or -tFlagVal)

### P2 features:

* Implement multiple broker sessions (ls and switch commands)
* Implement ncurses interface? This will let you subscribe to messages while running other commands.
* Implement command history? (pressing up and down will show last used commands)
* Refactor out as many dependencies as possible? (switch off from mqttrs library?)

## Research

* Rust is turning out to be quite a bear when it comes to creating a software architecture to solve a domain problem. The static typing and strict management of variable ownership and lifetimes means that a slight change or unexpected read/write access of a variable can completely change the architecture. Slight modifications to the functionality can completely shut off entire streams of possibilities. Some stack overflow advice for this is to start from the bottom up. Get something small working. And then another thing. And then integrate the pieces. And refactor repeatedly until the pieces fit together with something workable. Do that until the whole program is complete.

* Not sure what the velocity of coding in rust is, since I'm still pretty new. But so far it has been very slow. Trying to create a shell and state object and passing them around has had me run into lifetime issues and synchronization difficulties.

* The "keep alive" feature necessitated a somewhat large rewrite of the shell.rs file. This feature requires that the MQTT client sends a ping to the broker every n seconds in the background. The whole time, the user should be able to use the cli as if nothing were happening. Spawning a new thread for keep alive was initially tried but this too cumbersome because spawning a thread wherever you want makes it in \'static scope and this means you can't use references that don't have \'static lifetime (so I could not pass a reference to the shell, state, context, or command structs to the thread that needs those.). Next, mpsc channels were introduced to get around this problem. The shell.rs run() method was refactor to spawn 2 *scoped* threads, one that listens to user input and passes the input to the second thread, which parses the input and runs the commands in a loop. The channel to send commands to the "execution" thread was cloned and given to the keep alive thread, and it was that way the thread was able to send pings without having a reference to any of the structs. (This introduces an issue with exiting the program. See "Exiting the cli tool..." under **Open Issues**.). This may be more trouble than it actually is because of the way the program is architected. If a more idiomatic approach is taken with the entire program architecture, maybe this issue would go away? Purely an uneducated guess.

* Writing to a TcpStream is pretty straightforward. Reading from it is nontrivial. See "TcpStream" under "Resource and Links" for more info.

* Some Mqtt brokers implement a default topic called $SYS. Usually brokers are not supposed to make topics (this is done by clients), but this default topic is useful because brokers can broadcast system information like uptime on it. There is no standard and it's not part of the spec, so implementations are all different. See "$SYS Topics" under "Resources and Links" for a good article describing the intuition and details about it.

* Mqtt topics have a simple schema and a couple parts to them. When subscribing, you can use two types of wildcards ('\*' and '#'). Topics can be namespaced with the backslash. Like this: `mytopic/subtopic/additional_subtopic`. The asterisk wildcard can be used to greedily match topics. The pound sign stands in for only 1 sublevel. Wildcards are not permitted to be used during publish.
    * Other restrictions:
        1. Topics can not be the empty string. '/' is okay.
        1. Wildcards must be by themselves as the whole string, or between delimiters. For example, `mytopic/sub*topic/` is not allowed. But `mytopic/*/additional_topics` is allowed.
    * See "Understanding MQTT Topics" under "Resources and Links" for more information.

* I think I can attribute all the painfulness of learning rust to managing lifetimes. When you need to retroactively add in a lifetime, it changes the type of everything and then that propagates through the entire program. This is made worse if you design without regards to memory management, so maybe this is just a symptom of poor design rather than lifetimes being painful to use in general.

* On a related note, I've worked on this code base long enough to have a realization. Using the command pattern here was implicitly assuming that the shell needed to be modular and dynamically definable. These are really cool features and I wouldn't have too much of an issue writing this in C++, but it's kind of overkill. Probably a better way to do this is to just write a program (single-threaded, or at least with 2 threads with one being dedicated to reading stdin), and writing all the shell functionality as a cohesive unit. Then the cli package can be a thinner layer that contains an enum of what is now spec::Command. I hope this is clear enough to be understood. Perhaps this will be a good exercise if a v2 redesign is needed.

### Prior Art

This project will reference HiveMQ Mqtt-cli, which requires jdk and gradle to build, and does not have precompiled binaries for OpenBSD. Trying to build Mqtt-cli from source using gradle on OpenBSD results in errors because OpenBSD gradle does not support "SystemInfo".

Several other projects seemed promising but had no documentation and appeared to be early prototypes. (i.e. mqttc rust crate)

Other projects were not cli tools, which is what we need right now for testing.

## Resources and Links

1. [HiveMQ Mqtt-cli](https://github.com/hivemq/hivemq-mqtt-client)
1. [Rust-mq](https://github.com/inre/rust-mq)
1. [mqttc](https://docs.rs/mqttc/0.1.3/mqttc/)
1. [TcpStream](https://thepacketgeek.com/rust/tcpstream/reading-and-writing/)
1. [$SYS Topics](https://github.com/mqtt/mqtt.org/wiki/SYS-Topics)
1. [Understanding MQTT Topics](http://www.steves-internet-guide.com/understanding-mqtt-topics)
