use mqtt_cli::cli::shell::{self, Shell};
use mqtt_cli::cli::spec;
use mqtt_cli::commands;
use mqtt_cli::mqtt;

fn main() {
    let help = commands::help();
    let exit = commands::exit();
    let connect = commands::connect();
    let disconnect = commands::disconnect();
    let ping = commands::ping();
    let publish = commands::publish();
    let subscribe = commands::subscribe();

    let mut command_set = spec::CommandSet::new();
    command_set.insert(connect.name().to_owned(), connect);
    command_set.insert(disconnect.name().to_owned(), disconnect);
    command_set.insert(exit.name().to_owned(), exit);
    command_set.insert(help.name().to_owned(), help);
    command_set.insert(ping.name().to_owned(), ping);
    command_set.insert(publish.name().to_owned(), publish);
    command_set.insert(subscribe.name().to_owned(), subscribe);

    let context = mqtt::MqttContext {
        prompt_string: "mqtt".into(),
        client_id: "mqtt-cli".into(),
        username: None,
        broker: mqtt::BrokerAddr::new(),
        connection: None,
        keep_alive: None,
    };

    let shell = Shell::new(
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
