use mqtt_cli::cli::command::operand::error::MissingOperandError;
use mqtt_cli::cli::spec;
use mqtt_cli::cli::spec::flag;
use mqtt_cli::cli::shell::{self, Shell};
use mqtt_cli::commands;
use mqtt_cli::mqtt;

fn main() {
    let add = spec::Command::build("add")
        .set_help("Add two numbers together")
        .add_flag("verbose", 'v', spec::Arg::default(), "Print more info")
        .add_flag( "modulo", 'm', spec::Arg::Required, "Perform modulo on the resulting addition")
        .set_callback(| command, _shell, _state, _context | {
            let operands = command.operands();
            let expected_num_operands = 2;

            if operands.len() != expected_num_operands {
                return Err(Box::new(
                    MissingOperandError(
                        operands[..].into(),
                        expected_num_operands,
                    )
                ));
            }

            let res = operands[0].get_as::<i32>()? + operands[1].get_as::<i32>()?;
            let modulo =
                if let Some(modulo_flag) = command.get_flag(flag::Query::Short('m')) {
                    modulo_flag.arg().get_as::<i32>()?
                } else {
                    None
                };
            let res = if let Some(m) = modulo { res % m } else { res };
            println!("{}", res);

            Ok(spec::ReturnCode::Ok)
        });

    let help = commands::help();
    let exit = commands::exit();
    let connect = commands::connect();
    let ping = commands::ping();

    let mut command_set = spec::CommandSet::new();
    command_set.insert(add.name().to_owned(), add);
    command_set.insert(help.name().to_owned(), help);
    command_set.insert(exit.name().to_owned(), exit);
    command_set.insert(connect.name().to_owned(), connect);
    command_set.insert(ping.name().to_owned(), ping);

    let mut context = mqtt::MqttContext{
        prompt_string: "mqtt".into(),
        client_id: "mqtt-cli".into(),
        broker: mqtt::BrokerAddr::new(),
        connection: None,
        keep_alive: None,
        keep_alive_tx: None,
    };

    let shell = Shell::new(
        command_set,
        "This is an MQTT command line interface for testing MQTT implementations.\n\nCommands:",
    );

    let mut shell_state = shell::State::new();
    shell_state.insert(shell::STATE_ON_RUN_COMMAND.into(), "help".into());
    shell_state.insert(shell::STATE_PROMPT_STRING.into(), context.prompt_string.clone());

    shell.run(&mut shell_state, &mut context);
}
