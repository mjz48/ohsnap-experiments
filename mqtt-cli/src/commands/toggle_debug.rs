use crate::cli::shell;
use crate::cli::spec;
use crate::commands::util;
use crate::mqtt::MqttContext;

/// Send a ping request to the broker. This will return an error if the
/// client is not connected to anything.
pub fn toggle_debug() -> spec::Command<MqttContext> {
    spec::Command::build("debug")
        .set_description("Enter/exit debug mode. Gives access to debug commands.")
        .set_usage("{$name}")
        .set_callback(|_command, _shell, state, context| {
            context.debug = !context.debug;

            state.insert(
                shell::STATE_PROMPT_STRING.into(),
                shell::StateValue::RichString(util::generate_prompt(&context)),
            );

            Ok(spec::ReturnCode::Ok)
        })
}
