use crate::cli::spec::{self, flag};
use crate::mqtt::MqttContext;
use crate::tcp::{MqttPacketTx, PacketTx};
use mqttrs::{encode_slice, Connack, ConnectReturnCode, Packet};
use std::sync::mpsc::SendError;

fn string_to_connect_return_code(code: &str) -> ConnectReturnCode {
    match code {
        "RefusedProtocolVersion" => ConnectReturnCode::RefusedProtocolVersion,
        "RefusedIdentifierRejected" => ConnectReturnCode::RefusedIdentifierRejected,
        "ServerUnavailable" => ConnectReturnCode::ServerUnavailable,
        "BadUsernamePassword" => ConnectReturnCode::BadUsernamePassword,
        "NotAuthorized" => ConnectReturnCode::NotAuthorized,
        _ => ConnectReturnCode::Accepted,
    }
}

pub fn connack() -> spec::Command<MqttContext> {
    spec::Command::build("connack")
        .set_description("[Debug] Send a connack request to the broker. (This is illegal.)")
        .set_usage("{$name} {$flags}")
        .add_flag(
            "session-present",
            's',
            spec::Arg::None,
            "If this flag is present, the session-present bit will be set.",
        )
        .add_flag(
            "code",
            'c',
            spec::Arg::Required,
            "Specify the Connect Reason Code. Defaults to Accepted if not present.",
        )
        .set_enable(|_, _, _, context: &mut MqttContext| context.debug)
        .set_callback(|command, _shell, _state, context| {
            let session_present = command.get_flag(flag::Query::Short('s')).is_some();
            let code = command
                .get_flag(flag::Query::Short('c'))
                .and_then(|f| {
                    Some(string_to_connect_return_code(
                        f.arg().raw().unwrap_or("".into()).as_str(),
                    ))
                })
                .unwrap_or(ConnectReturnCode::Accepted);

            let pkt = Packet::Connack(Connack {
                session_present,
                code,
            });
            let mut buf = vec![0u8; 10];
            encode_slice(&pkt, &mut buf)?;

            if let Err(err) = context.tcp_send(PacketTx::Mqtt(MqttPacketTx {
                pkt: buf,
                keep_alive: true,
            })) {
                match err {
                    SendError(_) => {
                        return Err("No tcp connection to send ping.".into());
                    }
                }
            };

            Ok(spec::ReturnCode::Ok)
        })
}
