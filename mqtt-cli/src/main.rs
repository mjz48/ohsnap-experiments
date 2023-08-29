use mqtt_cli::cli::shell::{self, Shell};
use mqtt_cli::cli::spec;
use mqtt_cli::commands;
use mqtt_cli::mqtt;

fn main() {
    let help = commands::help();
    let exit = commands::exit();
    let connect = commands::connect();
    let connack = commands::connack();
    let disconnect = commands::disconnect();
    let ping = commands::ping();
    let publish = commands::publish();
    let subscribe = commands::subscribe();
    let unsubscribe = commands::unsubscribe();
    let toggle_debug = commands::toggle_debug();

    let mut command_set = spec::CommandSet::new();
    command_set.insert(connect.name().to_owned(), connect);
    command_set.insert(connack.name().to_owned(), connack);
    command_set.insert(disconnect.name().to_owned(), disconnect);
    command_set.insert(exit.name().to_owned(), exit);
    command_set.insert(help.name().to_owned(), help);
    command_set.insert(ping.name().to_owned(), ping);
    command_set.insert(publish.name().to_owned(), publish);
    command_set.insert(subscribe.name().to_owned(), subscribe);
    command_set.insert(unsubscribe.name().to_owned(), unsubscribe);
    command_set.insert(toggle_debug.name().to_owned(), toggle_debug);

    let context = mqtt::MqttContext {
        prompt_string: "mqtt".into(),
        debug: false,
        client_id: "mqtt-cli".into(),
        broker: mqtt::BrokerAddr::new(),
        keep_alive_tx: None,
        tcp_write_tx: None,
        tcp_read_rx: None,
    };

    let mut shell = Shell::new(
        command_set,
        "This is an MQTT command line interface for testing MQTT implementations.\n\nCommands:",
    );

    let mut shell_state = shell::State::new();
    shell_state.insert(
        shell::STATE_ON_RUN_COMMAND.into(),
        shell::StateValue::String("help".into()),
    );
    shell_state.insert(
        shell::STATE_PROMPT_STRING.into(),
        shell::StateValue::String(context.prompt_string.clone()),
    );

    shell.run(shell_state, context);
}
