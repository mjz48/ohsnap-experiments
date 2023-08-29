use crate::mqtt::MqttContext;
use colored::{ColoredString, Colorize};

pub fn generate_prompt(context: &MqttContext) -> ColoredString {
    if context.debug {
        format!(
            "{}@{}:{} [DEBUG]",
            context.client_id, context.broker.hostname, context.broker.port
        )
        .bright_red()
    } else {
        format!(
            "{}@{}:{}",
            context.client_id, context.broker.hostname, context.broker.port
        )
        .bright_yellow()
    }
}
